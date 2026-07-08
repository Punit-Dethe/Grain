# Reor: Deep Architectural Analysis

## Executive Summary

Reor is a sophisticated AI-powered personal knowledge management application built on Electron, React, and a local-first architecture. The core hypothesis is that **AI tools for thought should run models locally by default**. The application combines vector search, LLM-powered chat with RAG (Retrieval Augmented Generation), and a rich markdown editor to create an intelligent note-taking system that automatically links related content and provides semantic search capabilities.

**Key Technologies:**
- **Frontend**: React 18, TypeScript, TipTap/ProseMirror editor, Tamagui UI
- **Backend**: Electron (Node.js), LanceDB (vector database), Transformers.js (local embeddings)
- **LLM Integration**: Ollama (local), OpenAI API, Anthropic API
- **Vector Search**: LanceDB with Apache Arrow, cosine similarity
- **Embedding Models**: Transformers.js running ONNX models from HuggingFace

---

## 1. Application Architecture

### 1.1 Process Model (Electron)

Reor follows the standard Electron multi-process architecture:

#### Main Process (`electron/main/index.ts`)
- **Single instance enforcement** - prevents multiple app instances
- **Window management** via `WindowsManager` class
- **Ollama service initialization** - manages local LLM server
- **IPC handler registration** - 6 major subsystems:
  - LLM session handlers
  - Database session handlers
  - Store handlers (configuration persistence)
  - File system handlers
  - Electron utils handlers
  - Path handlers

- **Global error handling** - uncaught exceptions/rejections sent to Sentry (production) and UI
- **Hardware acceleration control** - disabled for specific OS versions (Windows 7, macOS 21.6)

**Critical Design Decision**: The app uses a custom `WindowsManager` class that maintains a mapping between browser windows and their associated vault directories and database table clients. This allows multiple windows to operate on different vaults simultaneously.

```typescript
type WindowInfo = {
  windowID: number
  dbTableClient: LanceDBTableWrapper
  vaultDirectoryForWindow: string
}
```

#### Renderer Process (`src/App.tsx`)
- **React application** with context-based state management
- **Initial setup flow** - ensures vault directory and embedding model are configured
- **Indexing progress tracking** - visual feedback during database population
- **PostHog analytics integration** - opt-in telemetry
- **Theme management** - dark mode default with system preference detection

#### Preload Script (`electron/preload/index.ts`)

**Security Bridge**: Uses `contextBridge` to expose safe IPC methods to renderer. Creates type-safe wrapper functions:

```typescript
function createIPCHandler<T extends (...args: any[]) => any>(channel: string): IPCHandler<T> {
  return (...args: Parameters<T>) => ipcRenderer.invoke(channel, ...args) as Promise<ReturnType<T>>
}
```

This pattern ensures:
1. No direct Node.js API exposure to renderer
2. Type safety across IPC boundaries
3. Single source of truth for API contracts

### 1.2 Directory Structure

```
reor-main/
├── electron/main/          # Main process (Node.js environment)
│   ├── common/            # Shared utilities (chunking, errors, network, window management)
│   ├── electron-store/    # Configuration persistence (electron-store)
│   ├── filesystem/        # File operations and watching (chokidar)
│   ├── llm/              # LLM management and Ollama integration
│   ├── path/             # Cross-platform path utilities
│   └── vector-database/  # LanceDB operations and embeddings
├── src/                   # Renderer process (Browser environment)
│   ├── components/       # React components
│   ├── contexts/         # React Context providers
│   ├── lib/              # Business logic, utilities, custom editor
│   └── styles/           # CSS and styling
└── shared/               # Code shared between main and renderer
```

---

## 2. Vector Database Architecture

### 2.1 LanceDB Implementation

**Why LanceDB?**
- **Column-oriented** Apache Arrow format for efficient vector operations
- **Embedded database** - no separate server process
- **Multi-modal support** - handles text, images, video
- **Built-in ANN search** - approximate nearest neighbor with IVF-PQ indexing


#### Database Schema (`vector-database/schema.ts`)

```typescript
interface DBEntry {
  notepath: string           // File path (sanitized for SQL)
  content: string            // Chunk text
  subnoteindex: number       // Position within file (0-based)
  timeadded: Date            // When indexed
  filemodified: Date         // File modification timestamp
  filecreated: Date          // File creation timestamp
  // vector: Float32[]       // Implicit - managed by LanceDB
}

interface DBQueryResult extends DBEntry {
  _distance: number          // Cosine distance (0=identical, 2=opposite)
}
```

**Critical Schema Design Decisions:**

1. **File Path Sanitization**: Single quotes in file paths are escaped (`'` → `''`) for SQL compatibility:
```typescript
sanitizePathForDatabase(filePath: string): string {
  return filePath.replace(/'/g, "''")
}
```

2. **Subnote Indexing**: Files are chunked, and each chunk maintains its position. This enables:
   - Reconstruction of context around a match
   - Deduplication during updates (delete all chunks for file, re-add)
   - Ordered retrieval of related content

3. **Timestamp Tracking**: Three temporal dimensions:
   - `timeadded`: When chunk was indexed (useful for debugging)
   - `filemodified`: Enables date-range filtering in searches
   - `filecreated`: Metadata for UI display


### 2.2 Table Management (`lanceTableWrapper.ts`)

**Key Architecture Pattern**: The `LanceDBTableWrapper` class abstracts LanceDB operations and provides domain-specific methods.

#### Table Naming Strategy
```typescript
generateTableName(embeddingFuncName: string, userDirectory: string): string {
  const sanitizeForFileSystem = (str: string) => str.replace(/[<>:"/\\|?*]/g, '_')
  return `ragnote_table_${sanitizedEmbeddingFuncName}_${sanitizedDirectory}`
}
```

**Why?** Different embedding models produce incompatible vectors (different dimensions). One table per (model, vault) combination ensures:
- No dimension mismatch errors
- Ability to switch models without data corruption
- Multi-vault support with model flexibility

#### Batch Insertion Strategy

```typescript
async add(data: DBEntry[], onProgress?: (progress: number) => void): Promise<void> {
  const numberOfChunksToIndexAtOnce = process.platform === 'darwin' ? 50 : 40
  // Chunks processed sequentially to avoid memory issues
  await chunks.reduce(async (previousPromise, chunk, index) => {
    await previousPromise
    const arrowTableOfChunk = makeArrowTable(chunk)
    await this.lanceTable.add(arrowTableOfChunk)
    onProgress?.((index + 1) / totalChunks)
  }, Promise.resolve())
}
```

**Critical Insights:**
1. **Platform-specific batch sizes**: macOS handles larger batches (50 vs 40)
2. **Sequential processing**: Prevents memory exhaustion on large vaults
3. **Progress callbacks**: Enables UI feedback during long operations
4. **Pre-deletion**: Before adding, existing chunks for that file are deleted (idempotent updates)


### 2.3 Embedding Pipeline (`embeddings.ts`)

#### Architecture: Transformers.js + ONNX Runtime

Reor runs **local embedding models** in the browser/Node.js via Transformers.js, which uses ONNX Runtime WebAssembly for inference.

**Default Models:**
```typescript
const defaultEmbeddingModelRepos = {
  'Xenova/UAE-Large-V1': {           // 1024-dim, English-optimized
    type: 'repo',
    description: 'Recommended for English content'
  },
  'Xenova/bge-small-en-v1.5': {     // 384-dim, lightweight
    type: 'repo', 
    description: 'Recommended for low-power devices'
  },
  'Xenova/multilingual-e5-large': { // 1024-dim, 100+ languages
    type: 'repo',
    description: 'Recommended for non-English content'
  },
  'Xenova/jina-embeddings-v2-base-zh': {  // Chinese-optimized
  'Xenova/jina-embeddings-v2-base-de': {  // German-optimized
}
```

**Model Loading Strategy:**

1. **Remote models** (HuggingFace Hub):
```typescript
async function createEmbeddingFunctionForRepo(config: EmbeddingModelWithRepo) {
  env.cacheDir = path.join(app.getPath('userData'), 'models', 'embeddings')
  env.allowRemoteModels = true
  
  try {
    pipe = await pipeline('feature-extraction', config.repoName)
  } catch (error) {
    // Fallback: manual download with custom fetch (proxy support)
    await DownloadModelFilesFromHFRepo(config.repoName, env.cacheDir)
    pipe = await pipeline('feature-extraction', config.repoName)
  }
}
```


2. **Local models** (custom paths):
```typescript
async function createEmbeddingFunctionForLocalModel(config: EmbeddingModelWithLocalPath) {
  const { localModelPath, repoName } = splitDirectoryPathIntoBaseAndRepo(config.localPath)
  env.localModelPath = localModelPath
  env.allowRemoteModels = false
  pipe = await pipeline('feature-extraction', repoName)
}
```

**Why both options?** 
- Remote: Easy onboarding, automatic updates
- Local: Air-gapped environments, custom fine-tuned models

#### Embedding Function Interface

```typescript
interface EnhancedEmbeddingFunction<T> extends lancedb.EmbeddingFunction<T> {
  name: string              // For table naming
  contextLength: number     // Vector dimensions (e.g., 1024)
  tokenize: (data: T[]) => string[]    // Pre-processing
  embed: (batch: T[]) => Promise<number[][]>  // Main inference
}
```

**Embedding Process:**
1. **Markdown stripping**: `removeMd(text)` removes formatting before embedding
2. **Mean pooling**: Token embeddings averaged to sentence embedding
3. **Normalization**: Vectors normalized to unit length (cosine similarity)

```typescript
const result = await pipe(removeMd(text), {
  pooling: 'mean',
  normalize: true,
})
return Array.from(result.data)  // Float32Array → number[]
```

---

## 3. Search Architecture

### 3.1 Vector Search (Semantic)

**Core Query Flow:**
```typescript
async search(query: string, limit: number, filter?: string): Promise<DBQueryResult[]> {
  const lanceQuery = await this.lanceTable
    .search(query)                    // LanceDB embeds query automatically
    .metricType(MetricType.Cosine)    // Cosine similarity
    .limit(limit)
  
  if (filter) {
    lanceQuery.prefilter(true)        // Apply filter before search (faster)
    lanceQuery.filter(filter)         // SQL-like: "filemodified > timestamp '...'"
  }
  
  return await lanceQuery.execute()
}
```


**Why Cosine Similarity?**
- Normalized vectors → only direction matters, not magnitude
- Efficient computation: dot product of unit vectors
- Range: 0 (identical) to 2 (opposite)
- Displayed as similarity: `(1 - distance) * 100%`

**Filtering System:**

Date-range filtering example:
```typescript
generateTimeStampFilter(minDate?: Date, maxDate?: Date): string {
  let filter = ''
  if (minDate) filter += `filemodified > timestamp '${formatISO(minDate)}'`
  if (maxDate) filter += ` AND filemodified < timestamp '${formatISO(maxDate)}'`
  return filter
}
```

This enables powerful queries like:
- "What did I work on last week?"
- "Show me notes from Q1 2024 about project X"

### 3.2 Hybrid Search (Vector + Keyword)

**Critical Innovation**: Reor implements a sophisticated hybrid search combining semantic and keyword matching.

#### Keyword Search Algorithm (`lib/db.ts`)

```typescript
const keywordSearch = async (query: string, limit: number, filter?: string) => {
  // 1. Extract meaningful keywords (stopword filtering)
  const keywords = query.toLowerCase()
    .split(/\s+/)
    .filter(word => word.length > 2 && !['the', 'and', 'for', ...].includes(word))
  
  // 2. Get vector results (for candidate set)
  const vectorResults = await window.database.search(query, limit, filter)
  
  // 3. Score candidates by keyword matches
  const escapedKeywords = keywords.map(k => k.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
  const keywordRegex = new RegExp(`\\b(${escapedKeywords.join('|')})\\b`, 'gi')
  
  return vectorResults.map(result => ({
    ...result,
    keyword_score: Array.from(result.content.matchAll(keywordRegex)).length
  })).sort((a, b) => (b.keyword_score || 0) - (a.keyword_score || 0))
}
```


**Why this approach?**
- Uses vector search as candidate generation (semantic similarity)
- Re-ranks by keyword frequency (precision for exact matches)
- Avoids full-text index (simplicity, no separate indexing)

#### Hybrid Scoring Algorithm

```typescript
combineAndRankResults(vectorResults, keywordResults, limit, vectorWeight = 0.7) {
  const keywordWeight = 1 - vectorWeight
  const resultsMap = new Map()
  
  // Normalize scores
  const maxKeywordScore = Math.max(...keywordResults.map(r => r.keyword_score || 0))
  
  // Add vector scores (weighted)
  vectorResults.forEach(result => {
    const vectorScore = 1 - result._distance  // Convert distance to similarity
    resultsMap.set(key, {
      ...result,
      combinedScore: vectorScore * vectorWeight
    })
  })
  
  // Add/boost with keyword scores (weighted)
  keywordResults.forEach(result => {
    const normalizedKeywordScore = result.keyword_score / maxKeywordScore
    const keywordComponent = normalizedKeywordScore * keywordWeight
    
    if (resultsMap.has(key)) {
      existingEntry.combinedScore += keywordComponent  // Boost existing
    } else {
      resultsMap.set(key, {
        ...result,
        combinedScore: keywordComponent  // New entry from keywords only
      })
    }
  })
  
  return Array.from(resultsMap.values())
    .sort((a, b) => b.combinedScore - a.combinedScore)
    .slice(0, limit)
}
```

**Configurable Balance:**
- `vectorWeight = 0.0`: Pure keyword search
- `vectorWeight = 0.5`: Balanced hybrid
- `vectorWeight = 0.7`: Default (semantic-leaning)
- `vectorWeight = 1.0`: Pure semantic search

This gives users fine-grained control via UI slider.

### 3.3 Re-ranking (Advanced)

**Optional secondary pass** using a cross-encoder model:


```typescript
const rerankSearchedEmbeddings = async (query: string, searchResults: DBEntry[]) => {
  const tokenizer = await AutoTokenizer.from_pretrained('Xenova/bge-reranker-base')
  const model = await AutoModelForSequenceClassification.from_pretrained('Xenova/bge-reranker-base')
  
  // Create query-document pairs
  const queries = Array(searchResults.length).fill(query)
  const inputs = tokenizer(queries, {
    text_pair: searchResults.map(item => item.content),
    padding: true,
    truncation: true,
  })
  
  // Score with cross-encoder
  const scores = await model(inputs)
  
  return searchResults
    .map((item, index) => ({ ...item, score: scores.logits.data[index] }))
    .sort((a, b) => b.score - a.score)
    .filter(item => item.score > 0)  // Threshold: positive relevance
}
```

**Bi-encoder vs Cross-encoder:**
- **Bi-encoder** (used for indexing): Encodes query and documents separately, fast but less accurate
- **Cross-encoder** (re-ranker): Encodes query+document together, slow but highly accurate

**Trade-off**: Re-ranking is computationally expensive, so it's applied to top-N results from bi-encoder search.

---

## 4. Text Chunking Strategy

### 4.1 Hierarchical Chunking (`common/chunking.ts`)

**Philosophy**: Preserve semantic boundaries (markdown headings) while handling large sections.

```typescript
export const chunkMarkdownByHeadingsAndByCharsIfBig = async (markdown: string) => {
  const chunkSize = store.get(StoreKeys.ChunkSize)  // Default: 1000 chars
  const chunkOverlap = 20
  
  // Phase 1: Split by headings
  const chunksByHeading = chunkMarkdownByHeadings(markdown)
  
  // Phase 2: Split oversized chunks recursively
  const oversizedChunks = chunksByHeading.filter(chunk => chunk.length > chunkSize)
  const rightSizedChunks = chunksByHeading.filter(chunk => chunk.length <= chunkSize)
  
  const recursivelyChunked = await chunkStringsRecursively(
    oversizedChunks, 
    chunkSize, 
    chunkOverlap
  )
  
  return [...rightSizedChunks, ...recursivelyChunked]
}
```


**Phase 1: Heading-based splitting**
```typescript
chunkMarkdownByHeadings(markdown: string): string[] {
  const lines = markdown.split('\n')
  const chunks: string[] = []
  let currentChunk: string[] = []

  lines.forEach(line => {
    if (line.startsWith('#')) {          // New heading = new chunk
      if (currentChunk.length) {
        chunks.push(currentChunk.join('\n'))
        currentChunk = []
      }
    }
    currentChunk.push(line)
  })
  
  if (currentChunk.length) chunks.push(currentChunk.join('\n'))
  return chunks
}
```

**Phase 2: Recursive character splitting**
Uses LangChain's `RecursiveCharacterTextSplitter`:
- Tries to split on: `\n\n` → `\n` → ` ` → character
- Maintains `chunkOverlap` to preserve context across boundaries
- Useful for large sections without headings (code blocks, long paragraphs)

**Design Rationale:**
1. **Heading preservation**: Keeps semantic units together (a section about "Database Design")
2. **Overlap strategy**: 20-char overlap prevents loss of context at boundaries
3. **Configurable size**: Users can adjust based on embedding model token limits
4. **Index tracking**: `subnoteindex` allows reconstruction of original document

---

## 5. LLM Integration Architecture

### 5.1 Multi-Provider System

**Architectural Pattern**: Provider abstraction layer with unified interface.

#### API Configuration Structure
```typescript
interface LLMAPIConfig {
  name: string                          // "OpenAI", "Anthropic", "Ollama"
  apiInterface: 'openai' | 'anthropic' | 'ollama'
  apiURL?: string                       // Base URL (customizable)
  apiKey?: string                       // Authentication
}

interface LLMConfig {
  modelName: string                     // "gpt-4o", "llama3.1:70b"
  apiName: string                       // Reference to LLMAPIConfig
  contextLength?: number                // Token limit
}
```


**This two-tier design allows:**
- Multiple models from same provider (OpenAI: gpt-4o, gpt-4o-mini, gpt-3.5-turbo)
- Easy provider switching (swap `apiName` reference)
- Custom API endpoints (local proxies, Azure OpenAI)

#### Client Resolution (`lib/llm/client.ts`)

```typescript
const resolveLLMClient = async (llmName: string): Promise<LanguageModel> => {
  const llmConfig = llmConfigs.find(llm => llm.modelName === llmName)
  const apiConfig = apiConfigs.find(api => api.name === llmConfig.apiName)
  
  switch (apiConfig.apiInterface) {
    case 'openai':
      return createOpenAI({
        apiKey: apiConfig.apiKey || '',
        baseURL: apiConfig.apiURL,
      })(llmName)
    
    case 'anthropic':
      return createAnthropic({
        apiKey: apiConfig.apiKey || '',
        baseURL: apiConfig.apiURL,
        headers: { 'anthropic-dangerous-direct-browser-access': 'true' }
      })(llmName)
    
    case 'ollama':
      return createOllama()(llmName)
  }
}
```

**Vercel AI SDK**: Provides unified streaming interface across providers.

### 5.2 Ollama Service Management

**Design Challenge**: Ollama needs to run as a local server. Reor manages this automatically.

#### Startup Logic (`llm/models/ollama.ts`)

```typescript
class OllamaService {
  async init() {
    const serveType = await this.serve()
    this.client = new Ollama()  // Connect to http://127.0.0.1:11434
  }
  
  async serve() {
    // 1. Check if already running
    try {
      await this.ping()
      return 'SYSTEM'  // User has Ollama installed and running
    } catch { /* continue */ }
    
    // 2. Try system-installed Ollama
    try {
      await this.execServe('ollama')
      return 'SYSTEM'
    } catch { /* continue */ }
    
    // 3. Use bundled binary
    const exePath = path.join(
      app.isPackaged ? process.resourcesPath : app.getAppPath(),
      'binaries',
      process.platform === 'darwin' ? 'ollama-darwin' : 
      process.platform === 'win32' ? 'ollama.exe' : 'ollama'
    )
    await this.execServe(exePath)
    return 'PACKAGED'
  }
}
```


**Graceful Degradation Hierarchy:**
1. Use running Ollama instance (user manages)
2. Start system-installed Ollama (common case)
3. Start bundled Ollama binary (air-gapped environments)

**Process Management:**
```typescript
async execServe(path: string) {
  return new Promise((resolve, reject) => {
    this.childProcess = exec(`"${path}" serve`, { env: process.env })
    
    this.waitForPing(1000, 20)  // Poll for 20 seconds
      .then(() => resolve(undefined))
      .catch((error) => {
        if (this.childProcess && !this.childProcess.killed) {
          this.childProcess.kill()
        }
        reject(error)
      })
  })
}

stop() {
  if (process.platform === 'win32') {
    exec(`taskkill /pid ${this.childProcess.pid} /f /t`)  // Force kill process tree
  } else {
    this.childProcess.kill()
  }
}
```

**Lifecycle Hooks:**
- `app.whenReady()` → `ollamaService.init()`
- `app.on('before-quit')` → `ollamaService.stop()`

### 5.3 Model Pulling Interface

```typescript
async pullModel(modelName: string, handleProgress: (chunk) => void): Promise<void> {
  const stream = await this.client.pull({
    model: modelName,
    stream: true,
  })
  
  for await (const progress of stream) {
    handleProgress(progress)  // Sent to renderer via IPC
  }
}
```

**UI Integration:**
```typescript
ipcMain.handle('pull-ollama-model', async (event, modelName: string) => {
  const handleProgress = (progress: ProgressResponse) => {
    event.sender.send('ollamaDownloadProgress', modelName, progress)
  }
  await ollamaService.pullModel(modelName, handleProgress)
  event.sender.send('llm-configs-changed')  // Trigger UI refresh
})
```

This enables real-time progress bars in the settings UI.

---

## 6. RAG (Retrieval Augmented Generation) System

### 6.1 Agent Configuration System

**Core Abstraction**: AgentConfig defines behavior for a chat session.


```typescript
type AgentConfig = {
  name: string                              // Display name
  dbSearchFilters?: DatabaseSearchFilters   // Automatic RAG
  files: string[]                           // Manual context files
  toolDefinitions: ToolDefinition[]         // Available tools
  promptTemplate: PromptTemplate            // System + user prompt
}

interface DatabaseSearchFilters {
  limit: number                   // Number of chunks to retrieve
  minDate?: Date                  // Temporal filtering
  maxDate?: Date
  passFullNoteIntoContext?: boolean  // Send full files vs chunks
}
```

**Design Philosophy**: 
- **Declarative**: Agent behavior specified upfront, not imperatively during chat
- **Composable**: Mix and match RAG, tools, custom prompts
- **Persistent**: Saved per-vault, reusable across sessions

#### Default Agent Configuration

```typescript
const defaultAgentPromptTemplate: PromptTemplate = [
  {
    role: 'system',
    content: `You are a helpful assistant responding to the user's query. 
You are operating within the context of the user's personal knowledge base.

Guidelines:
- Always respond in the same language as the user's query and context.
- You may be given context from the user's knowledge base that is relevant.
- The date and time of the query is {TODAY}.`
  },
  {
    role: 'user',
    content: `{QUERY}`
  }
]

const defaultAgent: AgentConfig = {
  name: 'Default',
  files: [],
  dbSearchFilters: {
    limit: 20,
    passFullNoteIntoContext: true  // Important: full files, not chunks
  },
  toolDefinitions: [],
  promptTemplate: defaultAgentPromptTemplate
}
```

**Key Decision: `passFullNoteIntoContext: true`**
- Retrieves top-20 semantic matches
- But sends *entire files* to LLM, not just matching chunks
- **Rationale**: Chunk boundaries can cut off important context; full files ensure completeness
- **Trade-off**: Higher token usage, but better quality responses

### 6.2 Context Retrieval Pipeline

```typescript
const retrieveContextItems = async (
  query: string, 
  agentConfig: AgentConfig
): Promise<DBEntry[] | FileInfoWithContent[]> => {
  
  // Priority 1: Manually specified files (highest relevance)
  if (agentConfig.files.length > 0) {
    return window.fileSystem.getFiles(agentConfig.files)
  }
  
  // Priority 2: Automatic RAG via vector search
  if (agentConfig.dbSearchFilters) {
    return retreiveFromVectorDB(query, agentConfig.dbSearchFilters)
  }
  
  // Priority 3: No context
  return []
}
```


**Priority Logic**: Manual files override automatic RAG. This allows:
- Focusing LLM on specific documents
- Ignoring irrelevant semantic matches
- Deterministic testing (same context every time)

### 6.3 Prompt Construction

```typescript
const generateMessagesFromTemplate = (
  promptTemplate: PromptTemplate,
  userQuery: string,
  contextItems: DBEntry[] | FileInfoWithContent[]
): ReorChatMessage[] => {
  
  return promptTemplate.map(templateMessage => {
    if (templateMessage.role === 'system') {
      return {
        ...templateMessage,
        content: replaceTemplatePlaceholders(templateMessage.content, userQuery),
        hideMessage: true  // Don't show in UI
      }
    }
    
    if (templateMessage.role === 'user') {
      return {
        ...templateMessage,
        context: contextItems,
        content: replaceTemplatePlaceholders(templateMessage.content, userQuery),
        visibleContent: userQuery  // Show original query in UI, not template
      }
    }
    
    return templateMessage
  })
}

const replaceTemplatePlaceholders = (content: string, userQuery: string) => {
  const today = format(new Date(), "yyyy-MM-dd'T'HH:mm:ss.SSSxxx")
  return content
    .replace('{QUERY}', userQuery)
    .replace('{TODAY}', today)
}
```

**Template Variables:**
- `{QUERY}`: User's input
- `{TODAY}`: Current timestamp (enables temporal reasoning)

### 6.4 Context Injection

```typescript
const injectContextStringIntoMessages = (
  messages: ReorChatMessage[],
  contextItems: DBEntry[] | FileInfoWithContent[],
  contextString: string
): ReorChatMessage[] => {
  
  const lastUserMessage = messages.findLast(msg => msg.role === 'user')
  
  if (lastUserMessage) {
    lastUserMessage.content = 
      `The context retrieved from the user's knowledge base for the query is: 
      ${contextString}
      
      ${lastUserMessage.content}`
    
    lastUserMessage.context = contextItems  // For UI display
  }
  
  return messages
}

const generateStringOfContextItemsForPrompt = (contextItems) => {
  return contextItems
    .map(item => JSON.stringify(item, null, 2))
    .join('\n\n')
}
```

**Critical Design Choice**: Context prepended to user message, not system message.

**Why?**
1. **Better attention**: LLMs attend more to recent tokens (user message closer to response)
2. **Provider compatibility**: Some APIs restrict system message format
3. **Token counting**: Easier to track context token usage


### 6.5 Chat Streaming Architecture

```typescript
const handleNewChatMessage = async (
  chat: Chat | undefined,
  llmName: string,
  userTextFieldInput?: string,
  agentConfig?: AgentConfig
) => {
  
  // 1. Create or extend chat with RAG context
  let outputChat = userTextFieldInput?.trim()
    ? await appendToOrCreateChat(chat, userTextFieldInput, agentConfig)
    : chat
  
  setCurrentChat(outputChat)
  await saveChat(outputChat)
  
  // 2. Resolve LLM client
  const llmClient = await resolveLLMClient(llmName)
  abortControllerRef.current = new AbortController()
  
  // 3. Convert tools to Zod schemas
  const toolsZodSchema = Object.assign({}, 
    ...outputChat.toolDefinitions.map(convertToolConfigToZodSchema)
  )
  
  setLoadingState('waiting-for-first-token')
  
  // 4. Stream response with Vercel AI SDK
  const { textStream, toolCalls } = await streamText({
    model: llmClient,
    messages: removeUncalledToolsFromMessages(outputChat.messages),
    tools: toolsZodSchema,
    abortSignal: abortControllerRef.current.signal,
  })
  
  // 5. Update UI incrementally
  for await (const text of textStream) {
    if (abortControllerRef.current.signal.aborted) return
    
    outputChat = {
      ...outputChat,
      messages: appendStringContentToMessages(outputChat.messages, text)
    }
    setCurrentChat(outputChat)
    setLoadingState('generating')
  }
  
  // 6. Handle tool calls
  if (!abortControllerRef.current.signal.aborted) {
    const { messages: outputMessages, allToolCallsHaveBeenExecuted } = 
      await appendToolCallsAndAutoExecuteTools(
        outputChat.messages,
        outputChat.toolDefinitions,
        await toolCalls
      )
    
    outputChat.messages = outputMessages
    setCurrentChat(outputChat)
    await saveChat(outputChat)
    
    // 7. Recursive call if tools need continuation
    if (allToolCallsHaveBeenExecuted) {
      handleNewChatMessage(outputChat, llmName, undefined, agentConfig)
    }
  }
  
  setLoadingState('idle')
}
```

**Key Architectural Patterns:**

1. **Optimistic UI Updates**: Chat saved immediately, before LLM response
2. **Streaming**: Token-by-token rendering for perceived performance
3. **Abort Control**: User can cancel mid-generation
4. **Tool Continuation**: Recursive calls enable multi-turn tool usage
5. **State Machine**: `idle` → `waiting-for-first-token` → `generating` → `idle`

---

## 7. Tool System (Agentic Behavior)

### 7.1 Tool Definition Schema

```typescript
type ToolDefinition = {
  name: string                    // Function identifier
  displayName?: string            // UI label
  description: string             // LLM-facing documentation
  parameters: ToolParameter[]     // Input schema
  autoExecute?: boolean           // Run without user approval
}

type ToolParameter = {
  name: string
  type: 'string' | 'number' | 'boolean'
  optional?: boolean
  defaultValue?: string | number | boolean
  description: string             // Help LLM provide correct arguments
}
```

### 7.2 Built-in Tool Implementations

#### Search Tool (Most Important)

```typescript
const searchToolDefinition: ToolDefinition = {
  name: 'search',
  description: `The search tool allows the LLM to automatically search your knowledge base. 
    It can filter by date & time, enabling queries like "what did I work on last week?"`,
  parameters: [
    { name: 'query', type: 'string', 
      description: 'Full user query for best results' },
    { name: 'limit', type: 'number', defaultValue: 20 },
    { name: 'minDate', type: 'string', optional: true, 
      description: 'ISO 8601 format: YYYY-MM-DDTHH:mm:ss.sssZ' },
    { name: 'maxDate', type: 'string', optional: true },
  ],
  autoExecute: true  // No confirmation needed
}

// Implementation
toolNamesToFunctions.search = async (
  query: string, 
  limit: number, 
  minDate: Date, 
  maxDate: Date
) => {
  return await retreiveFromVectorDB(query, { 
    limit, 
    minDate, 
    maxDate, 
    passFullNoteIntoContext: true 
  })
}
```

**Design Rationale:**
- **Auto-execute**: Search is read-only and safe
- **Date filtering**: Enables temporal reasoning without fine-tuning
- **Full context**: Returns entire files for comprehensive answers


#### File Manipulation Tools

```typescript
// Create Note
toolNamesToFunctions.createNote = async (filename: string, content: string) => {
  const vault = await window.electronStore.getVaultDirectoryForWindow()
  const path = await window.path.join(vault, filename)
  await window.fileSystem.createFile(path, content)
  return `Note ${path} created successfully`
}

// Edit Note
toolNamesToFunctions.editNote = async (filename: string, content: string) => {
  const vault = await window.electronStore.getVaultDirectoryForWindow()
  const path = await window.path.join(vault, filename)
  await window.fileSystem.writeFile({ filePath: path, content })
  return `Note ${filename} edited successfully`
}

// Append to Note
toolNamesToFunctions.appendToNote = async (filename: string, content: string) => {
  const vault = await window.electronStore.getVaultDirectoryForWindow()
  const path = await window.path.join(vault, filename)
  const currentContent = await window.fileSystem.readFile(path, 'utf-8')
  await window.fileSystem.writeFile({ 
    filePath: path, 
    content: currentContent + content 
  })
  return `Note ${filename} appended to successfully`
}

// Delete Note
toolNamesToFunctions.deleteNote = async (filename: string) => {
  const vault = await window.electronStore.getVaultDirectoryForWindow()
  const path = await window.path.join(vault, filename)
  await window.fileSystem.deleteFile(path)
  return `Note ${filename} deleted successfully`
}

// Read File
toolNamesToFunctions.readFile = async (filePath: string) => {
  return await window.fileSystem.readFile(filePath, 'utf-8')
}

// List Files
toolNamesToFunctions.listFiles = async () => {
  const files = await window.fileSystem.getFilesTreeForWindow()
  return files.map(file => file.name)
}

// Create Directory
toolNamesToFunctions.createDirectory = async (directoryName: string) => {
  const vault = await window.electronStore.getVaultDirectoryForWindow()
  const path = await window.path.join(vault, directoryName)
  await window.fileSystem.createDirectory(path)
  return `Directory ${directoryName} created successfully`
}
```

**Security Model**: 
- All operations scoped to current vault
- No absolute path manipulation from LLM
- File system access mediated through IPC
- No execution of arbitrary code

### 7.3 Tool Execution Pipeline

```typescript
const convertToolConfigToZodSchema = (tool: ToolDefinition) => {
  const parameterSchema = z.object(
    tool.parameters.reduce((acc, param) => {
      let zodType: z.ZodType<any>
      
      switch (param.type) {
        case 'string': zodType = z.string(); break
        case 'number': zodType = z.number(); break
        case 'boolean': zodType = z.boolean(); break
      }
      
      if (param.defaultValue !== undefined) {
        zodType = zodType.default(param.defaultValue)
      }
      if (param.optional) {
        zodType = zodType.optional()
      }
      
      zodType = zodType.describe(param.description)
      return { ...acc, [param.name]: zodType }
    }, {})
  )
  
  return {
    [tool.name]: {
      description: tool.description,
      parameters: parameterSchema,
    }
  }
}
```

**Why Zod?** 
- Runtime type validation (LLMs can hallucinate invalid types)
- Automatic JSON Schema generation for OpenAI function calling
- Type-safe tool arguments


#### Auto-execution Logic

```typescript
const autoExecuteTools = async (
  messages: ReorChatMessage[],
  toolDefinitions: ToolDefinition[],
  toolCalls: ToolCallPart[]
) => {
  
  // Filter to auto-executable tools
  const toolsThatNeedExecuting = toolCalls.filter(toolCall => {
    const toolDefinition = toolDefinitions.find(def => def.name === toolCall.toolName)
    return toolDefinition?.autoExecute
  })
  
  let outputMessages = messages
  const lastMessage = messages[messages.length - 1]
  
  // Execute each auto-executable tool
  for (const toolCall of toolsThatNeedExecuting) {
    outputMessages = await makeAndAddToolResultToMessages(
      outputMessages, 
      toolCall, 
      lastMessage
    )
  }
  
  const allToolCallsHaveBeenExecuted = 
    toolsThatNeedExecuting.length > 0 && 
    toolsThatNeedExecuting.length === toolCalls.length
  
  return { messages: outputMessages, allToolCallsHaveBeenExecuted }
}

const makeAndAddToolResultToMessages = async (
  messages: ReorChatMessage[],
  toolCallPart: ToolCallPart,
  assistantMessage: ReorChatMessage
) => {
  
  // Execute tool
  const toolResult = await createToolResult(
    toolCallPart.toolName, 
    toolCallPart.args, 
    toolCallPart.toolCallId
  )
  
  posthog.capture('tool_executed', { toolName: toolCallPart.toolName })
  
  // Create tool message
  const toolMessage: CoreToolMessage = {
    role: 'tool',
    content: [toolResult]
  }
  
  // Insert after assistant message
  const assistantIndex = messages.findIndex(msg => msg === assistantMessage)
  return [
    ...messages.slice(0, assistantIndex + 1),
    toolMessage,
    ...messages.slice(assistantIndex + 1)
  ]
}
```

**Message Flow:**
```
User: "What did I work on yesterday?"
  ↓
Assistant: [tool_call: search(query="work yesterday", minDate="2024-01-10T00:00:00", maxDate="2024-01-11T00:00:00")]
  ↓
Tool: [results: "Project X meeting notes...", "Code review feedback..."]
  ↓
Assistant: "Yesterday you worked on Project X meeting and code reviews..."
```

**Recursive Continuation**: If all tools auto-execute, the function calls itself to get final text response.

---

## 8. File System Management

### 8.1 File Watching with Chokidar

```typescript
const startWatchingDirectory = (win: BrowserWindow, directoryToWatch: string) => {
  try {
    const watcher = chokidar.watch(directoryToWatch, {
      ignoreInitial: true,  // Don't fire for existing files
    })
    
    const handleFileEvent = (eventType: string, filePath: string) => {
      if (fileHasExtensionInList(filePath, markdownExtensions) || 
          eventType.includes('directory')) {
        updateFileListForRenderer(win, directoryToWatch)
      }
    }
    
    watcher
      .on('add', path => handleFileEvent('added', path))
      .on('change', path => handleFileEvent('changed', path))
      .on('unlink', path => handleFileEvent('removed', path))
      .on('addDir', path => handleFileEvent('directory added', path))
      .on('unlinkDir', path => handleFileEvent('directory removed', path))
    
    return watcher
  } catch (error) {
    return undefined
  }
}
```

**Design Rationale:**
- **Real-time sync**: Changes from external editors (Obsidian, VS Code) reflected immediately
- **Filtered watching**: Only markdown files trigger updates (performance)
- **Directory awareness**: Folder operations update file tree
- **Error tolerance**: Gracefully handles permission issues


### 8.2 File Tree Construction

```typescript
function GetFilesInfoTree(pathInput: string, parentRelativePath = ''): FileInfoTree {
  const fileInfoTree: FileInfoTree = []
  
  if (!fs.existsSync(pathInput)) return fileInfoTree
  
  const stats = fs.statSync(pathInput)
  
  if (stats.isFile()) {
    // Base case: markdown file
    if (fileHasExtensionInList(pathInput, markdownExtensions) && 
        !isHidden(path.basename(pathInput))) {
      fileInfoTree.push({
        name: path.basename(pathInput),
        path: pathInput,
        relativePath: parentRelativePath,
        dateModified: stats.mtime,
        dateCreated: stats.birthtime,
      })
    }
  } else {
    // Recursive case: directory
    const itemsInDir = fs.readdirSync(pathInput)
      .filter(item => !isHidden(item))  // Skip .git, .obsidian, etc.
    
    const childNodes = itemsInDir
      .map(item => GetFilesInfoTree(
        path.join(pathInput, item),
        path.join(parentRelativePath, item)
      ))
      .flat()
    
    if (parentRelativePath === '') {
      return childNodes  // Root: return flat list
    }
    
    if (!isHidden(path.basename(pathInput))) {
      fileInfoTree.push({
        name: path.basename(pathInput),
        path: pathInput,
        relativePath: parentRelativePath,
        dateModified: stats.mtime,
        dateCreated: stats.birthtime,
        children: childNodes,  // Directory node
      })
    }
  }
  
  return fileInfoTree
}
```

**Data Structure:**
```typescript
type FileInfoNode = {
  name: string
  path: string
  relativePath: string
  dateModified: Date
  dateCreated: Date
  children?: FileInfoNode[]  // Only for directories
}
```

**Design Decisions:**
1. **Relative paths**: Enables vault portability (can move vault directory)
2. **Hidden file filtering**: Respects Unix conventions (. prefix)
3. **Recursive structure**: Natural tree representation for UI
4. **Lazy evaluation**: Tree built on-demand, not kept in memory

### 8.3 File Rename Handling

**Critical Challenge**: Renaming a file requires updating vector database paths.

```typescript
const handleFileRename = async (
  windowsManager: WindowsManager,
  windowInfo: { vaultDirectoryForWindow: string; dbTableClient: any },
  renameFileProps: RenameFileProps,
  sender: Electron.WebContents
) => {
  
  // 1. Stop watching (prevent infinite loops)
  windowsManager.watcher?.unwatch(windowInfo.vaultDirectoryForWindow)
  
  // 2. Check destination doesn't exist
  try {
    await fsPromises.access(renameFileProps.newFilePath)
    throw new Error(`File already exists: ${renameFileProps.newFilePath}`)
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
  
  // 3. Platform-specific handling
  if (process.platform === 'win32') {
    await windowsManager.watcher?.close()  // Windows needs full close
    
    await new Promise<void>((resolve, reject) => {
      fs.rename(renameFileProps.oldFilePath, renameFileProps.newFilePath, (err) => {
        if (err) { reject(err); return }
        
        const win = BrowserWindow.fromWebContents(sender)
        if (win) {
          windowsManager.watcher = startWatchingDirectory(win, windowInfo.vaultDirectoryForWindow)
          updateFileListForRenderer(win, windowInfo.vaultDirectoryForWindow)
        }
        resolve()
      })
    })
  } else {
    await fsPromises.rename(renameFileProps.oldFilePath, renameFileProps.newFilePath)
    windowsManager.watcher?.add(windowInfo.vaultDirectoryForWindow)
  }
  
  // 4. Update database
  await windowInfo.dbTableClient.updateDBItemsWithNewFilePath(
    renameFileProps.oldFilePath, 
    renameFileProps.newFilePath
  )
}
```

**Platform Quirks:**
- **Windows**: Requires full watcher close/restart (file system locking)
- **Unix**: Can unwatch/add specific paths (more granular)

#### Database Update

```typescript
async updateDBItemsWithNewFilePath(oldFilePath: string, newFilePath: string) {
  const sanitizedOldPath = sanitizePathForDatabase(oldFilePath)
  const filterString = `${DatabaseFields.NOTE_PATH} = '${sanitizedOldPath}'`
  
  await this.lanceTable.update({
    where: filterString,
    values: {
      [DatabaseFields.NOTE_PATH]: sanitizePathForDatabase(newFilePath)
    }
  })
}
```

**Why not delete + re-add?**
- Preserves timestamps (`timeadded`, `filemodified`)
- Faster for large files (no re-embedding)
- Maintains referential integrity (if other systems reference by path)

---

## 9. Editor Architecture

### 9.1 TipTap/ProseMirror Foundation

**Choice Rationale**: TipTap provides structured editing over plain textarea.

**Benefits:**
- **Document model**: Paragraphs, headings, lists as first-class nodes
- **Collaborative editing**: Operational transformation (unused but available)
- **Plugin architecture**: Custom behaviors via extensions
- **Markdown round-trip**: Parse markdown → ProseMirror → render markdown


#### Custom Block Schema

```typescript
export const hmBlockSchema: BlockSchema = {
  paragraph: defaultBlockSchema.paragraph,
  heading: defaultBlockSchema.heading,
  image: ImageBlock,
  'code-block': {
    propSchema: {
      ...defaultProps,
      language: { default: '' },
    },
    node: CodeBlockLowlight.configure({
      defaultLanguage: 'Plaintext',
      lowlight: createLowlight(common),  // Syntax highlighting
      languageClassPrefix: 'language-',
    }),
  },
  video: VideoBlock,
}
```

**Notable Exclusions**: 
- No bullet/numbered lists in custom schema (commented out)
- Likely future addition or architectural constraint

**Image/Video Blocks**: 
- Custom implementations with local storage
- Media stored in `userData` directory, not inline base64
- Enables large files without bloating markdown

### 9.2 Backlink System

**Core Feature**: `[[note name]]` syntax for wiki-style linking.

#### Decoration Plugin

```typescript
const backlinkPlugin = (updateSuggestionsState) => {
  return new Plugin({
    props: {
      decorations(state) {
        const decorations: Decoration[] = []
        const regex = /(\[\[)(.*?)(\]\])/g
        
        state.doc.descendants((node, pos) => {
          if (node.isText) {
            while (node.text) {
              const match = regex.exec(node.text)
              if (!match) break
              
              const start = pos + match.index
              const end = start + match[0].length
              const backlinkStart = start + match[1].length  // After [[
              const backlinkEnd = end - match[3].length      // Before ]]
              
              const withinSelectedRange = start <= selectionEnd && end >= selectionStart
              const bracketsStyle = withinSelectedRange 
                ? 'color: inherit;' 
                : 'display: none;'  // Hide brackets when not selected
              
              decorations.push(
                Decoration.inline(start, backlinkStart, { style: bracketsStyle }),
                Decoration.inline(backlinkEnd, end, { style: bracketsStyle }),
                Decoration.inline(backlinkStart, backlinkEnd, {
                  style: 'color: #92c8fc; text-decoration: underline; cursor: pointer;',
                  'data-backlink': 'true',
                })
              )
            }
          }
        })
        
        return DecorationSet.create(state.doc, decorations)
      }
    }
  })
}
```

**Styling Logic:**
- **Brackets**: Hidden unless cursor is inside link (cleaner appearance)
- **Link text**: Always visible, styled blue with underline
- **Clickable**: `cursor: pointer` indicates interactivity

#### Autocomplete System

```typescript
view() {
  return {
    update: (view) => {
      const { doc, selection } = view.state
      const { from } = selection
      
      if (!view.hasFocus()) {
        hideTimeout = setTimeout(() => updateSuggestionsState(null), 1000)
        return
      }
      
      const textBeforeCursor = doc.textBetween(0, from, '\n')
      const lastOpeningBracketIndex = textBeforeCursor.lastIndexOf('[[')
      
      // Not in backlink context
      if (lastOpeningBracketIndex === -1 || 
          textBeforeCursor.lastIndexOf(']]') > lastOpeningBracketIndex) {
        updateSuggestionsState(null)
        return
      }
      
      const textToLeft = textBeforeCursor.slice(lastOpeningBracketIndex + 2, from)
      const coords = view.coordsAtPos(from)
      
      updateSuggestionsState({
        textWithinBrackets: textToLeft,
        position: coords,
        onSelect: (selectedSuggestion) => {
          const { tr } = view.state
          const textAfterCursor = doc.textBetween(from, doc.content.size, '\n')
          const closingBracketIndex = textAfterCursor.indexOf(']]')
          
          if (closingBracketIndex !== -1) {
            tr.replaceWith(
              from - textToLeft.length,
              from + closingBracketIndex + 2,
              view.state.schema.text(`${selectedSuggestion}]]`)
            )
            view.dispatch(tr)
            updateSuggestionsState(null)
          }
        }
      })
    }
  }
}
```

**Autocomplete Trigger Logic:**
1. Check if cursor is inside `[[...]]` (unmatched opening bracket)
2. Extract text between `[[` and cursor
3. Filter file list by substring match
4. Show dropdown at cursor position
5. On select, replace entire `[[old text]]` with `[[new text]]`

#### Keyboard Shortcut

```typescript
handleDOMEvents: {
  keydown: (view, event) => {
    if (event.key === '[') {
      const { state, dispatch } = view
      const { selection } = state
      const { from } = selection
      
      // Auto-complete opening bracket to [[]]
      const transaction = state.tr.insertText(']', from)
      const newSelection = TextSelection.create(transaction.doc, from, from)
      transaction.setSelection(newSelection)
      dispatch(transaction)
      
      return true  // Prevent default
    }
    return false
  }
}
```

**UX Enhancement**: Typing `[` automatically inserts `]` and positions cursor between brackets.

### 9.3 File Saving Strategy

```typescript
const [needToWriteEditorContentToDisk, setNeedToWriteEditorContentToDisk] = useState(false)
const [needToIndexEditorContent, setNeedToIndexEditorContent] = useState(false)

const editor = useBlockNote({
  onEditorContentChange() {
    setNeedToWriteEditorContentToDisk(true)
    setNeedToIndexEditorContent(true)
  },
  // ...
})

const [debouncedNeedToWrite] = useDebounce(needToWriteEditorContentToDisk, 1000)

useEffect(() => {
  if (debouncedNeedToWrite) {
    writeEditorContentToDisk(editor, currentlyOpenFilePath)
    setNeedToWriteEditorContentToDisk(false)
  }
}, [debouncedNeedToWrite])
```

**Auto-save Strategy:**
1. Every keystroke sets `needToWrite` flag
2. Debounced hook triggers after 1 second of inactivity
3. Write to disk, clear flag
4. **Indexing**: Deferred until file switch or manual trigger (expensive operation)

**Trade-offs:**
- **Pro**: No data loss, real-time persistence
- **Con**: Frequent disk I/O (mitigated by debouncing)
- **Indexing delay**: Prevents constant re-embedding during active editing


---

## 10. State Management Architecture

### 10.1 Context-based Design

Reor uses React Context API instead of Redux/MobX. **Rationale**: Simpler for app-level state without complex async actions.

#### FileContext (`contexts/FileContext.tsx`)

**Responsibilities:**
- Current file path
- Editor instance
- File tree state (expanded directories)
- Navigation history
- Spell check settings
- Backlink suggestions state

```typescript
type FileContextType = {
  vaultFilesTree: FileInfoTree
  vaultFilesFlattened: FileInfo[]
  expandedDirectories: Map<string, boolean>
  handleDirectoryToggle: (path: string) => void
  currentlyOpenFilePath: string | null
  setCurrentlyOpenFilePath: (path: string | null) => void
  saveCurrentlyOpenedFile: () => Promise<void>
  editor: BlockNoteEditor | null
  navigationHistory: string[]
  addToNavigationHistory: (value: string) => void
  openOrCreateFile: (filePath: string, content?: string) => Promise<void>
  suggestionsState: SuggestionsState | null
  spellCheckEnabled: boolean
  renameFile: (oldPath: string, newPath: string) => Promise<void>
  deleteFile: (path: string) => Promise<boolean>
  selectedDirectory: string | null
}
```

**Key Pattern**: Context provides both state and actions (unlike Redux's separation).

#### ChatContext (`contexts/ChatContext.tsx`)

```typescript
type ChatContextType = {
  currentChat: Chat | undefined
  setCurrentChat: (chat: Chat | undefined) => void
  saveChat: (chat: Chat) => Promise<void>
  allChatsMetadata: ChatMetadata[]
  openNewChat: () => void
  deleteChat: (chatId: string) => Promise<void>
  activePanel: 'chat' | 'similarFiles' | null
  setActivePanel: (panel: 'chat' | 'similarFiles' | null) => void
}
```

**Chat Persistence:**
```typescript
const saveChat = async (chat: Chat) => {
  await window.electronStore.saveChat(chat)
  const metadata = await window.electronStore.getAllChatsMetadata()
  setAllChatsMetadata(metadata)
}
```

Chats stored per-vault in electron-store:
```typescript
store.get(StoreKeys.Chats) // { [vaultPath]: Chat[] }
```

#### ContentContext (`contexts/ContentContext.tsx`)

**Manages panel visibility:**
```typescript
type ContentContextType = {
  showEditor: boolean
  setShowEditor: (show: boolean) => void
  openContent: (path: string) => void
}
```

**Design Decision**: Separate context for UI state vs data state (FileContext). Enables:
- Independent re-renders
- Cleaner separation of concerns
- UI state doesn't pollute data contexts

### 10.2 IPC Event Bus

**Pattern**: Main process sends events to renderer for config changes.

```typescript
// Main process
ipcMain.handle('set-editor-flex-center', (event, value) => {
  store.set(StoreKeys.EditorFlexCenter, value)
  event.sender.send('editor-flex-center-changed', value)  // Broadcast
})

// Renderer process
useEffect(() => {
  const handleEditorChange = (event: any, editorFlexCenter: boolean) => {
    setEditorFlex(editorFlexCenter)
  }
  
  window.ipcRenderer.on('editor-flex-center-changed', handleEditorChange)
  
  return () => {
    // Cleanup listener
  }
}, [])
```

**Benefits:**
- Settings changes propagate to all windows
- No polling required
- Real-time synchronization

---

## 11. Performance Optimizations

### 11.1 Lazy Loading & Virtualization

**File Tree**: Uses `react-window` for large vaults:
```typescript
<FixedSizeList
  height={600}
  itemCount={flattenedFiles.length}
  itemSize={35}
  width="100%"
>
  {({ index, style }) => (
    <div style={style}>
      <FileItem file={flattenedFiles[index]} />
    </div>
  )}
</FixedSizeList>
```

**Why?** Vaults with 10,000+ files would render 10,000 DOM nodes. Virtualization renders only visible items (~20).

### 11.2 Debouncing Strategy

**Search Input:**
```typescript
const debouncedSearch = useCallback(
  (query: string) => {
    const debouncedFn = debounce(() => handleSearch(query), 300)
    debouncedFn()
  },
  [handleSearch]
)
```

**File Saving:**
```typescript
const [debouncedNeedToWrite] = useDebounce(needToWriteEditorContentToDisk, 1000)
```

**Rationale**: 
- Search: 300ms prevents search on every keystroke
- File save: 1000ms balances responsiveness vs disk I/O

### 11.3 Batch Database Operations

```typescript
const numberOfChunksToIndexAtOnce = process.platform === 'darwin' ? 50 : 40

const chunks = []
for (let i = 0; i < recordEntry.length; i += numberOfChunksToIndexAtOnce) {
  chunks.push(recordEntry.slice(i, i + numberOfChunksToIndexAtOnce))
}

await chunks.reduce(async (previousPromise, chunk, index) => {
  await previousPromise
  const arrowTableOfChunk = makeArrowTable(chunk)
  await this.lanceTable.add(arrowTableOfChunk)
  onProgress?.((index + 1) / totalChunks)
}, Promise.resolve())
```

**Memory Management**: 
- Processes 40-50 chunks sequentially (not all at once)
- Prevents OOM on large file imports
- Platform-tuned batch sizes (macOS handles more)

### 11.4 Memoization in Components

```typescript
const MemoizedFileItem = React.memo(FileItem, (prevProps, nextProps) => {
  return prevProps.file.path === nextProps.file.path &&
         prevProps.isSelected === nextProps.isSelected
})
```

**Selective Re-rendering**: Only re-render when file path or selection changes, not on tree expansion.

---

## 12. Security Architecture

### 12.1 Electron Security Best Practices

**Context Isolation Enabled:**
```typescript
const win = new BrowserWindow({
  webPreferences: {
    preload,
    contextIsolation: true,     // Default in Electron 12+
    nodeIntegration: false,     // Never enable
    sandbox: false,             // Needed for native modules
  }
})
```

**Why `sandbox: false`?**
- LanceDB uses native bindings (ONNX Runtime)
- Transformers.js needs Node.js APIs
- Trade-off: Performance vs sandboxing

### 12.2 IPC Security

**No Remote Code Execution:**
```typescript
// BAD (vulnerable)
ipcMain.handle('eval', (event, code) => {
  eval(code)  // NEVER DO THIS
})

// GOOD (Reor's approach)
ipcMain.handle('create-file', (event, filePath, content) => {
  // Validate inputs
  const vault = store.get(StoreKeys.DirectoryFromPreviousSession)
  if (!filePath.startsWith(vault)) {
    throw new Error('Path traversal attempt')
  }
  fs.writeFileSync(filePath, content)
})
```

**Principle**: 
- All IPC handlers validate inputs
- File paths scoped to vault
- No arbitrary code execution

### 12.3 Content Security Policy

```html
<meta http-equiv="Content-Security-Policy" 
      content="default-src 'self'; 
               script-src 'self'; 
               style-src 'self' 'unsafe-inline'; 
               img-src 'self' data: https:; 
               connect-src 'self' http://localhost:11434 https://api.openai.com https://api.anthropic.com;">
```

**Allows:**
- Self-hosted resources
- Inline styles (Tamagui requirement)
- Data URIs for images
- Localhost (Ollama)
- External LLM APIs

### 12.4 Secrets Management

**API Keys:**
```typescript
ipcMain.handle('add-or-update-llm-api-config', async (event, apiConfig: LLMAPIConfig) => {
  // API keys stored in electron-store (encrypted on disk)
  await addOrUpdateLLMAPIInStore(store, apiConfig)
})
```

**Electron-store encryption:**
- Uses OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- Keys encrypted at rest
- Not transmitted to renderer process (only used in main)

---

## 13. Cross-Platform Considerations

### 13.1 Path Handling

```typescript
// Platform-agnostic path operations
const pathSep = await window.path.pathSep()  // '/' or '\\'
const joined = await window.path.join(dir, file)
const absolute = await window.path.isAbsolute(path)
```

**Why async?** IPC calls to main process (where Node.js `path` module exists).

### 13.2 Platform-Specific Code

**File Watching:**
```typescript
if (process.platform === 'win32') {
  await windowsManager.watcher?.close()
  // Windows locks files, needs full watcher restart
} else {
  windowsManager.watcher?.unwatch(path)
  // Unix can selectively unwatch
}
```

**Batch Sizes:**
```typescript
const numberOfChunksToIndexAtOnce = process.platform === 'darwin' ? 50 : 40
// macOS handles larger batches
```

**Invalid Characters:**
```typescript
switch (platform) {
  case 'win32':
    invalidCharacters = /["*<>?|]/
    break
  case 'darwin':
    invalidCharacters = /[:]/
    break
  default:
    invalidCharacters = /$^/  // Linux: very permissive
}
```

### 13.3 Native Dependencies

**Optional Dependencies:**
```json
{
  "optionalDependencies": {
    "@lancedb/vectordb-darwin-x64": "^0.5.0",
    "@lancedb/vectordb-linux-x64-msvc": "0.5.0",
    "@lancedb/vectordb-win32-x64-msvc": "^0.5.0"
  }
}
```

**Why optional?** npm installs only the platform-specific native module, reducing bundle size.

---

## 14. Data Flow Diagrams

### 14.1 Indexing Pipeline

```
User opens vault
      ↓
Main process: Get vault directory + embedding model
      ↓
Initialize LanceDB connection
      ↓
Create/open table (named by model + vault)
      ↓
Scan directory tree (recursive)
      ↓
For each markdown file:
  ├─ Read file content
  ├─ Chunk by headings (+ recursive split if oversized)
  ├─ Remove markdown syntax
  ├─ Embed chunks (Transformers.js)
  ├─ Create DBEntry[] with metadata
  └─ Batch insert into LanceDB (40-50 at a time)
      ↓
Send progress updates to renderer
      ↓
Start file watcher (chokidar)
      ↓
Indexing complete
```

### 14.2 Search Flow

```
User enters query
      ↓
[Debounce 300ms]
      ↓
Determine search mode:
  ├─ Vector only (vectorWeight = 1.0)
  ├─ Hybrid (0.0 < vectorWeight < 1.0)
  └─ Keyword only (vectorWeight = 0.0)
      ↓
Vector Search:
  ├─ Embed query (same model as indexing)
  ├─ LanceDB cosine similarity search
  ├─ Apply filters (date range, file path)
  └─ Get top-N results with distances
      ↓
[If Hybrid]
Keyword Search:
  ├─ Extract keywords (stopword filtering)
  ├─ Regex match on vector results
  ├─ Count matches per document
  └─ Score by match frequency
      ↓
[If Hybrid]
Combine & Rank:
  ├─ Normalize scores (0-1 range)
  ├─ Weight: vectorScore * vectorWeight + keywordScore * (1-vectorWeight)
  ├─ Sort by combined score
  └─ Take top-N
      ↓
[Optional] Re-rank:
  ├─ Cross-encoder model (Xenova/bge-reranker-base)
  ├─ Score query+document pairs
  ├─ Filter by positive relevance
  └─ Re-sort
      ↓
Display results (similarity %, snippet, file name, modified date)
```


### 14.3 RAG Chat Flow

```
User submits message with AgentConfig
      ↓
Check if initial message or follow-up:
  ├─ Initial: Create new chat
  └─ Follow-up: Append to existing chat
      ↓
[If AgentConfig has dbSearchFilters]
RAG Context Retrieval:
  ├─ Vector search with user query
  ├─ Apply filters (limit, date range)
  ├─ [If passFullNoteIntoContext] Fetch full files
  └─ Format as JSON string
      ↓
[OR If AgentConfig has manual files]
  └─ Read specified files directly
      ↓
Construct Messages:
  ├─ System prompt (from template, {TODAY} replaced)
  ├─ User prompt: "Context: {contextString}\n\n{QUERY}"
  └─ Previous messages (if follow-up)
      ↓
Resolve LLM client (OpenAI/Anthropic/Ollama)
      ↓
Convert tools to Zod schemas
      ↓
Stream LLM response:
  ├─ Token-by-token text
  ├─ Update UI incrementally
  └─ Await tool calls
      ↓
[If tool calls present]
For each tool:
  ├─ Check if autoExecute flag set
  ├─ [If yes] Execute tool function
  ├─ Create tool result message
  └─ Append to message history
      ↓
[If all tools auto-executed]
Recursive call:
  └─ Get final LLM response with tool results
      ↓
Save chat to electron-store
      ↓
Display complete response
```

### 14.4 Similar Files (Graph) Flow

```
User opens file in editor
      ↓
Extract first 500 characters
      ↓
Remove markdown syntax
      ↓
Vector search (limit: 20)
  ├─ Embed chunk
  ├─ Cosine similarity
  └─ Filter: notepath != currentFile
      ↓
Sort by similarity (ascending distance)
      ↓
Display in sidebar:
  ├─ Content snippet (markdown rendered)
  ├─ Similarity percentage
  ├─ File name
  └─ Modification time
      ↓
User clicks result → Open file in editor → Repeat
```

**This creates the "graph" effect**: Each file leads to related files, forming a knowledge graph navigation experience.

---

## 15. Key Architectural Decisions & Trade-offs

### 15.1 Local-First Philosophy

**Decision**: All AI models run locally by default.

**Rationale:**
- **Privacy**: Notes never leave user's machine
- **Cost**: No API charges for daily use
- **Speed**: No network latency for embeddings/search
- **Offline**: Works without internet

**Trade-offs:**
- **Hardware requirements**: Needs decent CPU/RAM
- **Model quality**: Local models lag behind GPT-4/Claude
- **Setup complexity**: Users must download models

**Mitigation**: 
- Support for cloud LLMs (OpenAI, Anthropic) as opt-in
- Auto-download of embedding models
- Ollama integration simplifies local LLM management

### 15.2 LanceDB vs Alternatives

**Alternatives considered:**
- **Chroma**: More mature, but requires separate server
- **Pinecone/Weaviate**: Cloud-only, violates local-first
- **FAISS**: Low-level, no metadata filtering
- **Qdrant**: Heavier, Docker-based

**Why LanceDB:**
- **Embedded**: No server process
- **Arrow format**: Efficient columnar storage
- **SQL-like filtering**: Date ranges, path filters
- **Multi-modal**: Images/video support (future-proofing)

**Trade-offs:**
- **Maturity**: Newer than competitors
- **Documentation**: Less extensive
- **Native dependencies**: Platform-specific binaries

### 15.3 Chunking by Headings

**Alternative**: Fixed-size chunks (e.g., 512 tokens).

**Why heading-based:**
- **Semantic coherence**: Sections are topically unified
- **Variable size**: Respects content structure
- **Better retrieval**: Matches align with user's mental model

**Trade-offs:**
- **Uneven chunks**: Some sections huge, others tiny
- **Missing headings**: Falls back to character splitting (acceptable)

**Mitigation**: Recursive splitting for oversized sections.

### 15.4 Full Files vs Chunks in RAG

**Decision**: `passFullNoteIntoContext: true` sends entire files to LLM.

**Rationale:**
- **Context preservation**: Chunk boundaries can split important info
- **Better answers**: LLM sees full narrative arc
- **Simpler prompts**: No need to explain partial context

**Trade-offs:**
- **Token usage**: 20 full files can exceed context limits
- **Cost**: Higher API costs for cloud LLMs
- **Noise**: Irrelevant sections of files included

**Mitigation**: 
- Users can adjust `limit` (fewer files)
- Context limit checking (planned feature)
- Hybrid approach: chunks for search, files for RAG

### 15.5 Tool Auto-execution

**Decision**: Search tool auto-executes, others require confirmation.

**Rationale:**
- **Search**: Read-only, safe, frequent use case
- **File manipulation**: Destructive, needs user oversight

**Alternative**: All tools require confirmation (ChatGPT approach).

**Trade-off**: Less automation, but higher safety. Reor optimizes for power users who trust their LLM.

### 15.6 Electron vs Web-only

**Why Electron:**
- **File system access**: Direct vault manipulation
- **Native models**: Transformers.js needs Node.js
- **Offline-first**: No server infrastructure
- **OS integration**: Spotlight search, file watchers

**Trade-offs:**
- **Bundle size**: ~300MB download (includes native deps)
- **Performance**: Heavier than native apps
- **Updates**: User must download new versions

**Alternative considered**: Tauri (Rust + WebView)
- **Pros**: Smaller bundle, faster
- **Cons**: Less mature ecosystem, harder native module integration

---

## 16. Future Architecture Considerations

### 16.1 Scalability Bottlenecks

**Current limitations:**

1. **Single-threaded embedding**: Blocks main process during indexing
   - **Solution**: Worker threads or separate process
   
2. **In-memory file tree**: Large vaults (100k+ files) exhaust RAM
   - **Solution**: Paginated tree, lazy loading

3. **No incremental indexing**: File edits re-index entire file
   - **Solution**: Track chunk hashes, only re-embed changed chunks

4. **Global search**: Searches entire vault always
   - **Solution**: Scope search to folders/tags

### 16.2 Collaboration Features

**Current**: Single-user, local-only.

**Possible additions:**
- **Sync**: Git-based or custom protocol
- **Multiplayer editing**: CRDTs (Yjs integration in TipTap)
- **Shared vector DB**: Centralized LanceDB instance

**Challenges:**
- Embedding consistency across machines
- Conflict resolution in RAG context
- Privacy implications of centralized storage

### 16.3 Advanced RAG Techniques

**Not yet implemented:**

1. **Query expansion**: Generate multiple search queries from user input
2. **Contextual retrieval**: Re-rank based on current conversation
3. **Agentic loops**: LLM decides when to search vs use cached context
4. **Multi-hop reasoning**: Chain multiple searches for complex questions

**Why not now?**
- Complexity vs benefit trade-off
- Token cost considerations
- User control preferences (explicit > implicit)

### 16.4 Graph Visualization

**Current**: Implicit graph via similarity sidebar.

**Potential**:
- **Visual graph**: D3.js force-directed layout
- **Cluster analysis**: Identify topic communities
- **Path finding**: "How does note A relate to note B?"

**Technical requirements:**
- Pre-compute similarity matrix (expensive)
- Graph database layer (Neo4j?) or in-memory graph
- UI for large graphs (1000+ nodes)

---

## 17. Testing Strategy

### 17.1 Current Test Coverage

**Unit tests:**
- Path sanitization (`database.test.ts`)
- Schema validation
- File utilities

**Missing:**
- Embedding pipeline tests
- RAG flow tests
- UI component tests

**Rationale**: Early-stage project prioritizes features over test coverage.

### 17.2 Manual Testing Approach

**Key scenarios:**
1. **New vault setup**: Indexing progress, error handling
2. **Large files**: 10MB+ markdown files, chunking correctness
3. **Cross-platform**: Windows/macOS/Linux path handling
4. **Ollama integration**: Model pulling, server startup
5. **RAG accuracy**: Correct context retrieval for queries

### 17.3 Future Testing Infrastructure

**Needs:**
- **E2E tests**: Playwright for full workflows
- **Embedding tests**: Fixture-based (known documents → expected matches)
- **Performance tests**: Index 10k files, measure time/memory
- **Snapshot tests**: UI consistency across updates

---

## 18. Performance Benchmarks (Estimated)

**Hardware**: M1 Mac, 16GB RAM

| Operation | Time | Notes |
|-----------|------|-------|
| Index 100 notes (500KB total) | ~10s | UAE-Large-V1 model |
| Index 1000 notes (5MB total) | ~2min | Bottleneck: embedding |
| Vector search (1000-note vault) | ~50ms | LanceDB ANN search |
| Hybrid search (1000-note vault) | ~100ms | Vector + keyword re-rank |
| Chat response (first token) | ~1s | Local Llama 3.1 8B |
| Chat response (streaming) | ~20 tok/s | Local model |
| Chat response (GPT-4o) | ~200ms | Network + API latency |
| File save (auto-save) | ~10ms | Debounced, cached |
| Full app startup | ~2s | Ollama ping + DB connect |

**Bottlenecks:**
1. **Embedding inference**: CPU-bound, single-threaded
2. **Large file chunking**: Regex parsing on big documents
3. **UI rendering**: File tree with 10k+ files

---

## 19. Dependency Analysis

### 19.1 Critical Dependencies

| Package | Purpose | Risk |
|---------|---------|------|
| `vectordb` | LanceDB client | **High**: Core functionality |
| `@xenova/transformers` | Local embeddings | **High**: Local-first promise |
| `ollama` | Local LLM management | **Medium**: Can use APIs instead |
| `electron` | Desktop framework | **High**: Entire app architecture |
| `@tiptap/core` | Editor foundation | **High**: Note-taking UX |
| `ai` | Vercel AI SDK | **Medium**: Multi-provider abstraction |
| `chokidar` | File watching | **Medium**: Real-time sync |
| `electron-store` | Settings persistence | **Low**: Could use JSON files |

### 19.2 Unique Architectural Choices

1. **Transformers.js**: Unusual for desktop apps (typically Python/PyTorch)
   - **Benefit**: JavaScript ecosystem, WebAssembly performance
   - **Risk**: Limited model support vs PyTorch

2. **LanceDB**: Newer vector DB vs established options
   - **Benefit**: True embedding, no external server
   - **Risk**: Breaking changes, fewer resources

3. **TipTap**: Modern editor vs CKEditor/TinyMCE
   - **Benefit**: React integration, extensibility
   - **Risk**: Smaller community

---

## 20. Conclusion

### 20.1 Architectural Strengths

1. **Privacy-first**: No data leaves user's machine by default
2. **Modular design**: Clean separation (DB, LLM, filesystem, UI)
3. **Extensible**: Plugin architecture for tools, custom agents
4. **Performance**: Local embeddings fast enough for real-time search
5. **Cross-platform**: Single codebase for Windows/macOS/Linux

### 20.2 Technical Debt

1. **Test coverage**: Minimal automated testing
2. **Error handling**: Many silent failures (try/catch with comments)
3. **Type safety**: Some `any` types, implicit interfaces
4. **Documentation**: Inline comments sparse, no architecture docs (until now)
5. **Indexing efficiency**: No incremental updates, always full file

### 20.3 Innovation Points

1. **Hybrid search with configurable weighting**: Rare in PKM tools
2. **Tool auto-execution framework**: Enables agentic workflows
3. **Local-first RAG**: No cloud dependency for AI features
4. **Per-vault, per-model tables**: Elegant multi-model support
5. **Backlink autocomplete**: Obsidian-style UX in custom editor

### 20.4 Comparison to Alternatives

**vs Obsidian:**
- **Reor advantage**: Built-in AI, no plugins needed
- **Obsidian advantage**: Mature ecosystem, plugin marketplace

**vs Notion AI:**
- **Reor advantage**: Local-first, privacy, offline
- **Notion advantage**: Collaboration, cloud sync

**vs Mem.ai:**
- **Reor advantage**: Open source, customizable
- **Mem.ai advantage**: Mobile apps, polished UX

**vs Logseq:**
- **Reor advantage**: Vector search, RAG integration
- **Logseq advantage**: Outliner format, graph viz

### 20.5 Recommended Improvements

1. **Short-term**:
   - Add E2E tests for critical flows
   - Implement incremental indexing (hash-based)
   - Improve error messages (user-facing)
   - Add context window management (trim old messages)

2. **Medium-term**:
   - Worker threads for embedding (non-blocking)
   - Graph visualization UI
   - Mobile sync protocol
   - Plugin API for community extensions

3. **Long-term**:
   - Collaborative editing (Yjs)
   - Advanced RAG (query expansion, re-ranking)
   - Multi-modal search (images, PDFs)
   - Cloud-optional backend (self-hosted Reor server)

---

## 21. Technical Glossary

- **RAG**: Retrieval Augmented Generation - LLM + search context
- **Vector DB**: Database optimized for similarity search on embeddings
- **Embedding**: Numerical representation of text (e.g., 1024-dimensional vector)
- **Cosine similarity**: Measure of vector similarity (0-1, higher = more similar)
- **IPC**: Inter-Process Communication (Electron main ↔ renderer)
- **Chunking**: Splitting documents into smaller pieces for embedding
- **ANN**: Approximate Nearest Neighbor search (fast similarity search)
- **Bi-encoder**: Separate encoding of query and documents
- **Cross-encoder**: Joint encoding for more accurate re-ranking
- **Context window**: Maximum tokens an LLM can process
- **Tool calling**: LLM generating structured function calls
- **ONNX**: Open Neural Network Exchange (portable model format)
- **WebAssembly**: Low-level bytecode for near-native browser performance

---

**Document Version**: 1.0  
**Last Updated**: 2024 (analysis date)  
**Total Lines**: 1300+  
**Coverage**: Complete application stack (main process, renderer, database, LLM, UI)

