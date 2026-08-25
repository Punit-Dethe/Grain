// [GRAIN] Recommendation Lab worker. This is copied into ordinary unpacked
// projects; it has no Tauri access and runs through the production worker API.
(function () {
  var extensionId = "__GRAIN_LAB_EXTENSION_ID__";
  var configs = {
    "com.grain.lab.stream-music": {
      name: "Stream Music",
      mode: "confirm",
      commandDiagnostic: true,
      commands: [
        {
          id: "play-track",
          title: "Play a track",
          phrases: ["play track", "play song", "start track"],
          examples: [
            "put on Midnight City",
            "I want to hear one specific song",
            "start the track I just named",
          ],
        },
        {
          id: "play-playlist",
          title: "Play a playlist",
          phrases: ["play playlist", "start playlist", "put on playlist"],
          examples: [
            "put on my focus mix",
            "start the road trip collection",
            "play the list I made for dinner",
          ],
        },
        {
          id: "queue-track",
          title: "Queue a track",
          phrases: ["queue track", "add song to queue", "play this next"],
          examples: [
            "make Midnight City come on after this",
            "line up that song without interrupting the current one",
            "add this track as the next thing to hear",
          ],
        },
        {
          id: "start-artist-radio",
          title: "Start artist radio",
          phrases: ["artist radio", "play artist radio", "music like artist"],
          examples: [
            "keep playing things that sound like Daft Punk",
            "make me a station based on this artist",
            "continue with similar musicians",
          ],
        },
        {
          id: "resume-playback",
          title: "Resume playback",
          phrases: ["resume music", "continue playback", "keep playing"],
          examples: [
            "carry on from where the music stopped",
            "continue the song that was paused",
            "let the current track keep going",
          ],
        },
        {
          id: "skip-track",
          title: "Skip the current track",
          phrases: ["skip track", "next song", "play next"],
          examples: [
            "get this song out of here",
            "move on to whatever follows this",
            "I do not want to hear the rest of this track",
          ],
        },
        {
          id: "search-catalog",
          title: "Search the catalogue",
          phrases: ["search music", "find track", "find artist"],
          examples: [
            "see whether the service has this recording",
            "look for songs by this musician without playing them",
            "find the album but do not start it",
          ],
          autoSend: true,
        },
      ],
    },
    "com.grain.lab.music-library": {
      name: "Music Library",
      mode: "confirm",
      commandDiagnostic: true,
      commands: [
        {
          id: "play-local-track",
          title: "Play a local track",
          phrases: [
            "play local track",
            "play downloaded song",
            "play from library",
          ],
          examples: [
            "use the copy already saved on this computer",
            "play the downloaded version instead of streaming it",
            "start this song from my own collection",
          ],
        },
        {
          id: "find-in-library",
          title: "Find in library",
          phrases: ["search library", "find local music", "look up album"],
          examples: [
            "check whether I already own this album",
            "look through my collection without playing anything",
            "find the local recording by this artist",
          ],
          autoSend: true,
        },
        {
          id: "add-to-library",
          title: "Add to library",
          phrases: ["add to library", "save track", "keep album"],
          examples: [
            "keep this record in my personal collection",
            "save the currently playing song to my music",
            "make this album part of my library",
          ],
        },
        {
          id: "add-to-playlist",
          title: "Add to a playlist",
          phrases: ["add to playlist", "save to playlist", "put in playlist"],
          examples: [
            "put this track in the focus mix",
            "save the current song inside my road trip list",
            "include this recording in a playlist",
          ],
        },
        {
          id: "import-files",
          title: "Import music files",
          phrases: ["import music", "add music files", "scan folder"],
          examples: [
            "bring the audio from Downloads into my collection",
            "scan this folder for records I have not added",
            "register these files with the music library",
          ],
        },
        {
          id: "edit-metadata",
          title: "Edit track metadata",
          phrases: ["edit metadata", "fix track title", "change album artist"],
          examples: [
            "correct the artist name on this recording",
            "fix the album information for these songs",
            "change the title stored on the local file",
          ],
        },
        {
          id: "show-recent",
          title: "Show recently added",
          phrases: ["recent music", "recently added", "new in library"],
          examples: [
            "what did I add to my collection lately",
            "show the newest albums in my library",
            "list the music imported this week",
          ],
          autoSend: true,
        },
      ],
    },
    "com.grain.lab.issue-tracker": {
      name: "Issue Tracker",
      mode: "issue-form",
      commandDiagnostic: true,
      commands: [
        {
          id: "create-issue",
          title: "Create an issue",
          phrases: ["create issue", "new ticket", "report bug"],
          examples: [
            "capture a new bug for the login crash",
            "open work to investigate the broken checkout",
            "record this as a fresh project problem",
          ],
        },
        {
          id: "find-issues",
          title: "Find issues",
          phrases: ["search issues", "find ticket", "list issues"],
          examples: [
            "show the existing bugs about failed logins",
            "look for related project work without changing it",
            "which tickets mention the checkout service",
          ],
          autoSend: true,
        },
        {
          id: "update-status",
          title: "Update issue status",
          phrases: ["update issue status", "move ticket", "mark in progress"],
          examples: [
            "move the login bug into active development",
            "show that work has started on this ticket",
            "change the workflow state of the issue",
          ],
        },
        {
          id: "assign-issue",
          title: "Assign an issue",
          phrases: ["assign issue", "give ticket to", "change assignee"],
          examples: [
            "make Maya responsible for the login bug",
            "hand this ticket over to the platform team",
            "put the issue in Jordan's queue",
          ],
        },
        {
          id: "comment-on-issue",
          title: "Comment on an issue",
          phrases: ["comment on issue", "add ticket comment", "reply to issue"],
          examples: [
            "tell everyone on the ticket that the fix shipped",
            "leave an update on the login bug",
            "add these investigation notes to the issue discussion",
          ],
        },
        {
          id: "set-priority",
          title: "Set issue priority",
          phrases: ["set issue priority", "prioritize ticket", "mark urgent"],
          examples: [
            "make the checkout bug the most urgent work",
            "lower this ticket to normal importance",
            "change how urgently the project should handle this issue",
          ],
        },
        {
          id: "link-issues",
          title: "Link related issues",
          phrases: ["link issues", "relate tickets", "mark duplicate"],
          examples: [
            "connect this bug to the earlier login report",
            "show that these two tickets describe the same failure",
            "associate the current issue with its blocking work",
          ],
        },
        {
          id: "close-issue",
          title: "Close an issue",
          phrases: ["close issue", "resolve ticket", "finish bug"],
          examples: [
            "mark this bug finished now that the fix is live",
            "resolve the ticket without adding another comment",
            "take this completed issue out of active work",
          ],
        },
      ],
    },
    "com.grain.lab.code-host": {
      name: "Code Host",
      mode: "confirm",
      commandDiagnostic: true,
      commands: [
        {
          id: "open-pull-request",
          title: "Open a pull request",
          phrases: ["open pull request", "create pr", "propose branch"],
          examples: [
            "put this branch up for review",
            "propose merging my current changes",
            "start a code review request from this branch",
          ],
        },
        {
          id: "review-pull-request",
          title: "Review a pull request",
          phrases: ["review pull request", "review pr", "inspect changes"],
          examples: [
            "go through the changes in the open proposal",
            "inspect this branch before it gets merged",
            "start reviewing the latest code request",
          ],
        },
        {
          id: "merge-pull-request",
          title: "Merge a pull request",
          phrases: ["merge pull request", "merge pr", "land changes"],
          examples: [
            "land the approved change on the main branch",
            "combine this reviewed proposal into the repository",
            "finish the pull request by merging it",
          ],
        },
        {
          id: "inspect-checks",
          title: "Inspect build checks",
          phrases: ["show checks", "inspect build", "view ci status"],
          examples: [
            "tell me why the latest validation failed",
            "show the build status without rerunning anything",
            "which automated checks are blocking this change",
          ],
          autoSend: true,
        },
        {
          id: "create-repository-issue",
          title: "Create a repository issue",
          phrases: [
            "create repository issue",
            "open code issue",
            "report repo bug",
          ],
          examples: [
            "record a bug directly against this repository",
            "open a code-host issue for the failing parser",
            "create a repository ticket rather than a project task",
          ],
        },
        {
          id: "compare-branches",
          title: "Compare branches",
          phrases: [
            "compare branches",
            "show branch diff",
            "changes between branches",
          ],
          examples: [
            "show what differs between release and main",
            "inspect the delta without opening a pull request",
            "what changed from this branch to production",
          ],
          autoSend: true,
        },
        {
          id: "search-code",
          title: "Search repository code",
          phrases: ["search code", "find in repository", "look up symbol"],
          examples: [
            "find every place that calls the old parser",
            "look through this repository for the auth constant",
            "locate the symbol without changing any code",
          ],
          autoSend: true,
        },
        {
          id: "create-branch",
          title: "Create a branch",
          phrases: ["create branch", "new branch", "branch from main"],
          examples: [
            "start a new line of work from main",
            "make a branch for the parser fix",
            "create a repository branch for this task",
          ],
        },
      ],
    },
    "com.grain.lab.translator": {
      name: "Translator - Multilingual",
      mode: "direct",
      commandDiagnostic: true,
      commands: [
        {
          id: "translate-selection",
          title: "Translate selected text",
          phrases: [
            "translate selection",
            "translate selected text",
            "convert selection",
          ],
          examples: [
            "turn the highlighted paragraph into Spanish",
            "convert what I selected to English",
            "translate only the text that is currently highlighted",
          ],
        },
        {
          id: "translate-message",
          title: "Translate a message",
          phrases: [
            "translate message",
            "translate this",
            "say in another language",
          ],
          examples: [
            "make this note sound natural in French",
            "how would I send this sentence in Japanese",
            "convert my short message into German",
          ],
        },
        {
          id: "detect-language",
          title: "Detect the language",
          phrases: ["detect language", "identify language", "what language"],
          examples: [
            "tell me which language this paragraph uses",
            "identify the language without translating the text",
            "what language was this message written in",
          ],
          autoSend: true,
        },
        {
          id: "transliterate-text",
          title: "Transliterate text",
          phrases: [
            "transliterate text",
            "romanize text",
            "write phonetically",
          ],
          examples: [
            "write these Japanese sounds using Latin letters",
            "show how to pronounce this with the English alphabet",
            "romanize the script but keep the same language",
          ],
          autoSend: true,
        },
        {
          id: "define-phrase",
          title: "Define a phrase",
          phrases: ["define phrase", "explain meaning", "what does this mean"],
          examples: [
            "explain the meaning of this expression",
            "what does this foreign phrase mean in context",
            "define these words rather than translating the whole paragraph",
          ],
          autoSend: true,
        },
        {
          id: "localize-tone",
          title: "Localize tone",
          phrases: [
            "localize tone",
            "adapt translation",
            "make culturally natural",
          ],
          examples: [
            "rewrite this translation so it sounds polite in Japan",
            "adapt the message for a casual French audience",
            "keep the meaning but make the wording culturally natural",
          ],
        },
      ],
    },
    "com.grain.lab.calendar": {
      name: "Calendar Planner",
      mode: "calendar-form",
      commands: [
        "create",
        "move",
        "cancel",
        "availability",
        "agenda",
        "invite",
        "decline",
        "accept",
        "remind",
        "search",
        "focus-time",
      ],
    },
    "com.grain.lab.team-chat": {
      name: "Team Chat",
      mode: "message-form",
      commands: [
        "channel-message",
        "direct-message",
        "reply",
        "edit",
        "delete",
        "react",
        "pin",
        "unpin",
        "save",
        "search",
        "unread",
        "status",
        "remind",
      ],
    },
    "com.grain.lab.email": {
      name: "Email Assistant",
      mode: "message-form",
      commands: [
        "compose",
        "reply",
        "reply-all",
        "forward",
        "archive",
        "delete",
        "label",
        "star",
        "snooze",
        "search",
        "draft",
        "schedule",
        "unsubscribe",
        "summarize",
      ],
    },
    "com.grain.lab.meeting-notes": {
      name: "Meeting Notes",
      mode: "long-result",
      commands: [
        "minutes",
        "decisions",
        "actions",
        "summary",
        "attendees",
        "risks",
        "follow-ups",
      ],
    },
    "com.grain.lab.document-notes": {
      name: "Document Notes",
      mode: "long-result",
      commands: [
        "create",
        "append",
        "find",
        "move",
        "tag",
        "link",
        "archive",
        "outline",
        "export",
      ],
    },
    "com.grain.lab.task-list": {
      name: "Task List",
      mode: "task-form",
      commands: [
        "create",
        "complete",
        "reopen",
        "schedule",
        "prioritize",
        "assign",
        "label",
        "move",
        "list",
        "search",
      ],
    },
    "com.grain.lab.knowledge-base": {
      name: "Knowledge Base Research and Documentation Assistant",
      mode: "long-result",
      commands: [
        "search",
        "summarize",
        "open",
        "cite",
        "compare",
        "recent",
        "owner",
        "related",
      ],
    },
    "com.grain.lab.weather": {
      name: "Weather",
      mode: "direct",
      commands: ["current", "hourly", "daily", "rain", "wind"],
    },
    "com.grain.lab.maps": {
      name: "Maps and Directions",
      mode: "confirm",
      commands: [
        "navigate",
        "route",
        "nearby",
        "traffic",
        "transit",
        "walk",
        "cycle",
        "save-place",
        "share-place",
      ],
    },
    "com.grain.lab.contacts": {
      name: "Contacts",
      mode: "contact-form",
      commands: [
        "find",
        "create",
        "update",
        "merge",
        "delete",
        "company",
        "favorites",
      ],
    },
    "com.grain.lab.calculator": {
      name: "Calculator",
      mode: "direct",
      commands: [
        "arithmetic",
        "percent",
        "unit",
        "currency",
        "split",
        "interest",
        "date",
        "statistics",
      ],
    },
    "com.grain.lab.browser-search": {
      name: "Web Search",
      mode: "direct",
      commands: ["search", "news", "images", "shopping", "academic"],
    },
    "com.grain.lab.file-organizer": {
      name: "File Organizer",
      mode: "danger",
      commands: [
        "find",
        "rename",
        "move",
        "copy",
        "delete",
        "organize",
        "compress",
        "extract",
        "duplicate",
        "recent",
        "large",
        "type",
        "tag",
        "open",
        "reveal",
      ],
    },
    "com.grain.lab.timer": {
      name: "Timer",
      mode: "completion",
      commands: ["start", "pause", "resume", "cancel"],
    },
    "com.grain.lab.smart-home": {
      name: "Smart Home",
      mode: "danger",
      commands: [
        "light-on",
        "light-off",
        "brightness",
        "temperature",
        "scene",
        "lock",
        "unlock",
        "garage",
        "fan",
        "camera",
        "alarm",
        "music",
        "vacuum",
        "blinds",
        "sprinkler",
        "status",
        "energy",
        "away",
      ],
    },
    "com.grain.lab.video-meeting": {
      name: "Video Meeting",
      mode: "confirm",
      commands: [
        "start",
        "join",
        "invite",
        "link",
        "mute",
        "camera",
        "share",
        "record",
      ],
    },
    "com.grain.lab.clipboard-tools": {
      name: "Clipboard Tools",
      mode: "direct",
      commands: ["plain-text", "markdown", "list", "clean", "case", "inspect"],
    },
    "com.grain.lab.expense-tracker": {
      name: "Expense Tracker",
      mode: "expense-form",
      commands: [
        "record",
        "categorize",
        "receipt",
        "reimburse",
        "split",
        "budget",
        "weekly",
        "monthly",
        "merchant",
        "search",
        "export",
        "recurring",
      ],
    },
    "com.grain.lab.travel-planner": {
      name: "Travel Planner",
      mode: "travel-form",
      commands: [
        "itinerary",
        "flight",
        "train",
        "hotel",
        "restaurant",
        "activity",
        "route",
        "budget",
        "packing",
        "visa",
        "weather",
        "calendar",
        "share",
        "compare",
      ],
    },
  };

  // Each generated worker executes only one profile. Drop the other 23 fixture
  // objects immediately so the stress corpus does not become retained worker
  // state merely because this single-file lab template contains it.
  var selectedConfig = configs[extensionId] || null;
  configs = null;
  var active = null;
  var COMMAND_FLOOR = 0.5;
  var COMMAND_ASK_MARGIN = 0.08;
  var COMMAND_AUTO_SEND_MARGIN = 0.15;

  function phrases(id) {
    return String(id)
      .split("-")
      .concat([String(id).replace(/-/g, " ")]);
  }

  function commandId(command) {
    return typeof command === "string" ? command : command.id;
  }

  function commandTitle(command) {
    return typeof command === "string"
      ? String(command).replace(/-/g, " ")
      : command.title;
  }

  function commandById(config, id) {
    for (var index = 0; index < config.commands.length; index += 1) {
      if (commandId(config.commands[index]) === id)
        return config.commands[index];
    }
    return null;
  }

  function clampScore(value) {
    var score = Number(value);
    if (!Number.isFinite(score)) return 0;
    return Math.max(0, Math.min(1, score));
  }

  function scoreMap(ranked) {
    var scores = Object.create(null);
    (ranked || []).forEach(function (entry) {
      if (entry && typeof entry.id === "string") {
        scores[entry.id] = clampScore(entry.score);
      }
    });
    return scores;
  }

  function hasScore(scores, id) {
    return Object.prototype.hasOwnProperty.call(scores, id);
  }

  function buildDiagnosticRanking(config, lexical, semantic) {
    var lexicalScores = scoreMap(lexical);
    var semanticScores = semantic === null ? null : scoreMap(semantic);
    var rows = config.commands.map(function (command) {
      var id = commandId(command);
      var lexicalScore = hasScore(lexicalScores, id) ? lexicalScores[id] : null;
      var semanticScore =
        semanticScores !== null && hasScore(semanticScores, id)
          ? semanticScores[id]
          : null;
      // Semantic is the confidence baseline. A lexical hit contributes only a
      // bounded corroboration bonus; it cannot lift a below-floor semantic
      // candidate onto the decision ballot or make anything Auto-sendable.
      var combinedScore =
        semanticScore === null
          ? lexicalScore || 0
          : clampScore(
              semanticScore + (1 - semanticScore) * 0.2 * (lexicalScore || 0),
            );
      return {
        id: id,
        title: commandTitle(command),
        command: command,
        lexicalScore: lexicalScore,
        semanticScore: semanticScore,
        combinedScore: combinedScore,
      };
    });
    rows.sort(function (left, right) {
      return (
        right.combinedScore - left.combinedScore ||
        left.id.localeCompare(right.id)
      );
    });
    return rows;
  }

  async function diagnoseCommand(request, config) {
    var lexicalCandidates = config.commands.map(function (command) {
      return {
        id: command.id,
        phrases: command.phrases,
      };
    });
    var semanticCandidates = config.commands.map(function (command) {
      return {
        id: command.id,
        examples: command.examples,
      };
    });
    var lexical = [];
    var lexicalAvailable = true;
    try {
      lexical = await grain.match.lexical(request, lexicalCandidates);
    } catch (_) {
      lexicalAvailable = false;
      // Lexical is supporting evidence. A host failure is visible in the
      // missing score column but must not hide a valid semantic decision.
    }
    var semantic = null;
    try {
      semantic = await grain.match.semantic(request, semanticCandidates);
    } catch (_) {
      // Model absence/failure is an honest degraded state: show lexical
      // diagnostics, execute nothing, and never pretend a confidence.
    }
    var ranked = buildDiagnosticRanking(config, lexical, semantic);
    if (semantic === null) {
      return {
        state: "unavailable",
        ranked: ranked,
        suggested: [],
        pick: null,
        autoSend: false,
        selectedByUser: false,
        lexicalAvailable: lexicalAvailable,
        semanticAvailable: false,
      };
    }

    var decisionCandidates = ranked
      .filter(function (row) {
        return row.semanticScore !== null && row.semanticScore >= COMMAND_FLOOR;
      })
      .map(function (row) {
        return { id: row.id, score: row.combinedScore };
      });
    var decision = await grain.match.decide(decisionCandidates, {
      minConfidence: COMMAND_FLOOR,
      margin: COMMAND_ASK_MARGIN,
    });
    var semanticAutoDecision = await grain.match.decide(
      (semantic || []).map(function (entry) {
        return { id: entry.id, score: clampScore(entry.score) };
      }),
      {
        minConfidence: COMMAND_FLOOR,
        margin: COMMAND_AUTO_SEND_MARGIN,
      },
    );
    if (decision && typeof decision.pick === "string") {
      var picked = commandById(config, decision.pick);
      return {
        state: "executed",
        ranked: ranked,
        suggested: [],
        pick: decision.pick,
        autoSend: Boolean(
          picked &&
            picked.autoSend === true &&
            semanticAutoDecision &&
            semanticAutoDecision.pick === decision.pick,
        ),
        selectedByUser: false,
        lexicalAvailable: lexicalAvailable,
        semanticAvailable: true,
      };
    }
    if (decision && Array.isArray(decision.ambiguous)) {
      return {
        state: "suggested",
        ranked: ranked,
        suggested: decision.ambiguous.slice(0, 3),
        pick: null,
        autoSend: false,
        selectedByUser: false,
        lexicalAvailable: lexicalAvailable,
        semanticAvailable: true,
      };
    }
    return {
      state: "unresolved",
      ranked: ranked,
      suggested: [],
      pick: null,
      autoSend: false,
      selectedByUser: false,
      lexicalAvailable: lexicalAvailable,
      semanticAvailable: true,
    };
  }

  async function chooseCommand(request, config) {
    var candidates = config.commands.map(function (command) {
      var id = commandId(command);
      return { id: id, phrases: phrases(id) };
    });
    var ranked = await grain.match.lexical(request, candidates);
    if (ranked && ranked.length) return ranked[0].id;
    return commandId(config.commands[0]);
  }

  function percent(score) {
    return score === null ? "—" : (clampScore(score) * 100).toFixed(1) + "%";
  }

  function diagnosticStatus(analysis) {
    if (analysis.state === "executed") {
      return analysis.selectedByUser
        ? "The user resolved an ambiguous command. The lab now simulates that command executing."
        : "The extension found one command above the semantic floor and clear of the runner-up.";
    }
    if (analysis.state === "suggested") {
      return "The leading commands are within the 8-point decision margin. Choose one to test the clarification transition.";
    }
    if (analysis.state === "unavailable") {
      return "Semantic matching is unavailable. Lexical scores remain visible, but the extension executes nothing in this degraded state.";
    }
    return "No command cleared the 50% semantic floor. The extension refuses to guess.";
  }

  function diagnosticBadges(analysis) {
    var badges = [];
    if (analysis.state === "executed") {
      badges.push({ type: "badge", text: "Executed", tone: "success" });
      if (analysis.autoSend) {
        badges.push({ type: "badge", text: "Auto-send", tone: "info" });
      }
      if (analysis.selectedByUser) {
        badges.push({ type: "badge", text: "User selected", tone: "neutral" });
      }
    } else if (analysis.state === "suggested") {
      badges.push({ type: "badge", text: "Suggested", tone: "warning" });
    } else if (analysis.state === "unavailable") {
      badges.push({
        type: "badge",
        text: "Semantic unavailable",
        tone: "danger",
      });
    } else {
      badges.push({ type: "badge", text: "Unresolved", tone: "muted" });
    }
    return badges;
  }

  function diagnosticView(config, request, analysis) {
    var rankedRows = analysis.ranked.slice(0, 5).map(function (row, index) {
      return {
        type: "metadata",
        label: String(index + 1) + ". " + row.title,
        value:
          "Combined " +
          percent(row.combinedScore) +
          " · semantic " +
          percent(row.semanticScore) +
          " · lexical " +
          percent(row.lexicalScore),
      };
    });
    var actions = [];
    if (analysis.state === "suggested") {
      actions = analysis.suggested.map(function (id, index) {
        var command = commandById(config, id);
        return {
          id: "choose-" + id,
          label: command ? command.title : id,
          intent: index === 0 ? "primary" : "secondary",
          kind: "submit",
        };
      });
    } else {
      actions.push({
        id: "finish-diagnostic",
        label:
          analysis.state === "executed" ? "Finish test" : "Close diagnostic",
        intent: "primary",
        kind: "submit",
      });
    }
    actions.push({
      id: "cancel-diagnostic",
      label: "Cancel",
      intent: "cancel",
      kind: "cancel",
    });
    return {
      version: 1,
      title: config.name + " command decision",
      description:
        "Same-window Recommendation Lab instrumentation. No external action is performed.",
      root: {
        type: "stack",
        gap: "md",
        children: [
          {
            type: "section",
            title: "Decision",
            children: [
              {
                type: "inline",
                gap: "sm",
                align: "start",
                wrap: true,
                children: diagnosticBadges(analysis),
              },
              { type: "text", text: diagnosticStatus(analysis) },
            ],
          },
          {
            type: "section",
            title: "Ranked internal commands",
            children: rankedRows,
          },
          {
            type: "section",
            title: "Evidence policy",
            children: [
              {
                type: "metadata",
                label: "Signals",
                value:
                  "Semantic " +
                  (analysis.semanticAvailable ? "available" : "unavailable") +
                  " · lexical " +
                  (analysis.lexicalAvailable ? "available" : "unavailable"),
              },
              {
                type: "metadata",
                label: "Thresholds",
                value:
                  "Semantic floor 50.0% · ask margin 8.0% · Auto-send margin 15.0%",
              },
              {
                type: "metadata",
                label: "Auto-send rule",
                value:
                  "Safe command + semantic clear winner. Lexical evidence can never enable it.",
              },
              {
                type: "metadata",
                label: "Original request",
                value: request.slice(0, 2048),
              },
            ],
          },
        ],
      },
      actions: actions,
    };
  }

  function metadata(request, command) {
    return [
      { type: "metadata", label: "Resolved command", value: command },
      {
        type: "metadata",
        label: "Original request",
        value: request.slice(0, 2048),
      },
    ];
  }

  function confirmView(config, request, command, danger) {
    return {
      version: 1,
      title: danger ? "Review consequential action" : "Confirm request",
      description:
        config.name +
        " resolved one of " +
        config.commands.length +
        " internal commands.",
      root: {
        type: "stack",
        gap: "md",
        children: [
          {
            type: "section",
            title: config.name,
            children: metadata(request, command),
          },
          {
            type: "text",
            tone: danger ? "warning" : "muted",
            text: danger
              ? "This lab action is drawn with Grain's destructive-action treatment. It does not touch real data."
              : "The fixture waits for explicit confirmation before returning its result.",
          },
        ],
      },
      actions: [
        {
          id: "confirm",
          label: danger ? "Run test action" : "Confirm",
          intent: danger ? "danger" : "primary",
          kind: "submit",
        },
        { id: "cancel", label: "Cancel", intent: "cancel", kind: "cancel" },
      ],
    };
  }

  function formView(config, request, command) {
    var children = metadata(request, command);
    if (config.mode === "issue-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "title",
          label: "Issue title",
          value: request.slice(0, 120),
          required: true,
          maxLength: 120,
        },
        {
          type: "text_area",
          id: "details",
          label: "Description",
          value: request,
          rows: 5,
          maxLength: 2000,
        },
        {
          type: "select",
          id: "priority",
          label: "Priority",
          value: "medium",
          options: [
            { value: "low", label: "Low" },
            { value: "medium", label: "Medium" },
            { value: "high", label: "High" },
            { value: "urgent", label: "Urgent" },
          ],
        },
        {
          type: "checkbox",
          id: "notify",
          label: "Notify the project team",
          checked: true,
        },
      ]);
    } else if (config.mode === "calendar-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "title",
          label: "Event title",
          value: request.slice(0, 100),
          required: true,
          maxLength: 100,
        },
        {
          type: "text_field",
          id: "when",
          label: "Date and time",
          value: "Tomorrow at 10:00",
          required: true,
          maxLength: 80,
        },
        {
          type: "text_area",
          id: "attendees",
          label: "Attendees",
          placeholder: "Names or email addresses",
          rows: 3,
          maxLength: 500,
        },
      ]);
    } else if (config.mode === "message-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "recipient",
          label: "Recipient",
          placeholder: "Person, channel, or address",
          required: true,
          maxLength: 120,
        },
        {
          type: "text_area",
          id: "message",
          label: "Message",
          value: request,
          rows: 6,
          required: true,
          maxLength: 2000,
        },
      ]);
    } else if (config.mode === "task-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "task",
          label: "Task",
          value: request.slice(0, 160),
          required: true,
          maxLength: 160,
        },
        {
          type: "select",
          id: "priority",
          label: "Priority",
          value: "normal",
          options: [
            { value: "normal", label: "Normal" },
            { value: "high", label: "High" },
          ],
        },
      ]);
    } else if (config.mode === "contact-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "name",
          label: "Name",
          required: true,
          maxLength: 120,
        },
        { type: "text_field", id: "email", label: "Email", maxLength: 200 },
        { type: "text_field", id: "phone", label: "Phone", maxLength: 80 },
      ]);
    } else if (config.mode === "expense-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "amount",
          label: "Amount",
          placeholder: "0.00",
          required: true,
          maxLength: 32,
        },
        {
          type: "select",
          id: "category",
          label: "Category",
          value: "meals",
          options: [
            { value: "meals", label: "Meals" },
            { value: "travel", label: "Travel" },
            { value: "software", label: "Software" },
            { value: "other", label: "Other" },
          ],
        },
        {
          type: "checkbox",
          id: "reimbursable",
          label: "Reimbursable",
          checked: false,
        },
      ]);
    } else {
      children = children.concat([
        {
          type: "text_field",
          id: "destination",
          label: "Destination",
          required: true,
          maxLength: 120,
        },
        {
          type: "text_area",
          id: "preferences",
          label: "Preferences",
          value: request,
          rows: 5,
          maxLength: 1600,
        },
      ]);
    }
    return {
      version: 1,
      title: config.name,
      description:
        "Editable Grain-rendered form from a Recommendation Lab extension.",
      root: {
        type: "stack",
        gap: "md",
        children: [
          { type: "section", title: "Request details", children: children },
        ],
      },
      actions: [
        {
          id: "submit",
          label: "Complete test",
          intent: "primary",
          kind: "submit",
        },
        { id: "cancel", label: "Cancel", intent: "cancel", kind: "cancel" },
      ],
    };
  }

  grain.ui.onEvent(function (event) {
    if (!active) return;
    if (event.kind === "cancel") {
      active = null;
      return;
    }
    if (event.kind === "change") return;
    if (active.kind === "command-diagnostic") {
      if (event.target.indexOf("choose-") === 0) {
        var selected = event.target.slice("choose-".length);
        if (active.analysis.suggested.indexOf(selected) < 0) {
          active = null;
          return {
            error: "The selected command was not offered by this view.",
          };
        }
        active.analysis = {
          state: "executed",
          ranked: active.analysis.ranked,
          suggested: [],
          pick: selected,
          autoSend: false,
          selectedByUser: true,
          lexicalAvailable: active.analysis.lexicalAvailable,
          semanticAvailable: active.analysis.semanticAvailable,
        };
        return {
          view: diagnosticView(active.config, active.request, active.analysis),
        };
      }
      if (event.target !== "finish-diagnostic") {
        active = null;
        return { error: "The command diagnostic received an unknown action." };
      }
      var diagnostic = active;
      active = null;
      if (diagnostic.analysis.state === "executed") {
        var resolved = commandById(diagnostic.config, diagnostic.analysis.pick);
        return {
          message:
            diagnostic.config.name +
            " completed the simulated '" +
            (resolved ? resolved.title : diagnostic.analysis.pick) +
            "' command." +
            (diagnostic.analysis.autoSend
              ? " It satisfied the semantic-only Auto-send policy."
              : " It did not Auto-send."),
        };
      }
      return {
        message:
          diagnostic.config.name +
          " closed the diagnostic without executing an internal command.",
      };
    }
    var values = event.values || {};
    var fields = Object.keys(values)
      .map(function (key) {
        return key + "=" + String(values[key]);
      })
      .join(", ");
    var message =
      active.config.name +
      " completed '" +
      active.command +
      "' in the Recommendation Lab.";
    if (fields) message += "\n\nSubmitted values: " + fields;
    active = null;
    return { message: message };
  });

  grain.onRequest(async function (request) {
    active = null;
    var config = selectedConfig;
    if (!config) return { error: "Recommendation Lab profile is missing" };
    var lower = request.toLowerCase();
    if (
      extensionId === "com.grain.lab.travel-planner" &&
      (lower.indexOf("navigate") >= 0 || lower.indexOf("directions") >= 0)
    ) {
      return {
        decline:
          "This is a point-to-point route; Maps and Directions is a better owner.",
      };
    }
    if (
      extensionId === "com.grain.lab.file-organizer" &&
      lower.indexOf("protected") >= 0
    ) {
      return {
        error:
          "The lab refused a protected-file operation to exercise the terminal failure surface.",
      };
    }
    if (config.commandDiagnostic === true) {
      try {
        var analysis = await diagnoseCommand(request, config);
        active = {
          kind: "command-diagnostic",
          config: config,
          request: request,
          analysis: analysis,
        };
        return { view: diagnosticView(config, request, analysis) };
      } catch (_) {
        active = null;
        return {
          error:
            "The internal command diagnostic could not complete safely. Retry after checking the semantic model.",
        };
      }
    }
    var command = await chooseCommand(request, config);
    if (config.mode === "direct") {
      return {
        message:
          config.name +
          " handled '" +
          command +
          "' directly, without a confirmation surface.\n\nOriginal request: " +
          request,
      };
    }
    if (config.mode === "long-result") {
      return {
        message:
          config.name +
          " produced a finite result for '" +
          command +
          "'.\n\nThis deliberately longer second paragraph exercises wrapping, Copy, Insert, and Replace without opening an extension-owned conversation. The original request remains visible here for verification: " +
          request,
      };
    }
    if (config.mode === "completion") return {};
    active = {
      kind: "legacy-lab",
      config: config,
      request: request,
      command: command,
    };
    if (config.mode === "confirm")
      return { view: confirmView(config, request, command, false) };
    if (config.mode === "danger")
      return { view: confirmView(config, request, command, true) };
    return { view: formView(config, request, command) };
  });
})();
