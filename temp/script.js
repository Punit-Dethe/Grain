const card = document.getElementById('agent-card');
const input = document.getElementById('agent-input');
const sendBtn = document.getElementById('send-btn');
const waveGrid = document.getElementById('wave-grid');

let isExpanded = false;

// Initialize Wave Grid
const WAVE_ROWS = 4;
const WAVE_COLS = 12;
const dots = [];

for (let i = 0; i < WAVE_ROWS * WAVE_COLS; i++) {
    const r = Math.floor(i / WAVE_COLS);
    const c = i % WAVE_COLS;
    const isCorner = (r === 0 || r === WAVE_ROWS - 1) && (c === 0 || c === WAVE_COLS - 1);
    
    const dot = document.createElement('span');
    dot.className = 'agent-wave-dot';
    
    // Dynamic Dark Yellow to Orange Gradient
    // Left (Dark Yellow): rgb(210, 160, 0)
    // Right (Orange): rgb(255, 85, 0)
    const ratio = c / (WAVE_COLS - 1); // 0 at left, 1 at right
    // Linear transition is best for two rich colors
    const r_val = Math.round(210 + (45 * ratio)); // 210 to 255
    const g_val = Math.round(160 - (75 * ratio)); // 160 to 85
    const b_val = 0;
    dot.style.backgroundColor = `rgb(${r_val}, ${g_val}, ${b_val})`;

    if (isCorner) {
        dot.style.visibility = 'hidden';
    }
    waveGrid.appendChild(dot);
    dots.push(dot);
}

// Start Wave Animation
const start = performance.now();
function tick(now) {
    const time = (now - start) / 1000; // Use seconds for easier math
    
    for (let i = 0; i < dots.length; i++) {
        const c = i % WAVE_COLS;
        const r = Math.floor(i / WAVE_COLS);
        const isCorner = (r === 0 || r === WAVE_ROWS - 1) && (c === 0 || c === WAVE_COLS - 1);
        
        if (!isCorner) {
            // "Quantum Audio Stream" Animation (Right to Left)
            // Using time * positive_speed + c * positive_freq forces the wave to travel LEFT.
            
            // Wave 1: Fast, tight ripples carrying the high frequencies
            const w1 = Math.sin(c * 0.6 + time * 6.0 + r * 0.5);
            
            // Wave 2: Slower, wider swells that modulate the fast ripples (creates organic "beats")
            const w2 = Math.sin(c * 0.3 + time * 3.0 - r * 1.2);
            
            // Wave 3: A pulsing energy that shifts vertically across the rows
            const w3 = Math.cos(c * 0.8 + time * 4.5 + r * 2.0);
            
            // Additive and Amplitude synthesis (literally how real audio works)
            const combined = (w1 * 0.5) + (w2 * w1 * 0.5) + (w3 * 0.3);
            
            // Map the complex signal to opacity (0.05 to 1.0)
            const normalized = (combined + 1.3) / 2.6; // Maps roughly -1.3..1.3 to 0..1
            const opacity = 0.05 + (normalized * 0.95);
            
            dots[i].style.opacity = Math.max(0.05, Math.min(1.0, opacity));
        }
    }
    requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

// Keyboard interactions
window.addEventListener('keydown', (e) => {
    // If already expanded
    if (isExpanded) {
        if (e.key === 'Tab') {
            e.preventDefault(); // Prevent tab from moving focus off page
            // Shrink back to recording
            isExpanded = false;
            card.classList.remove('is-expanded');
            input.value = '';
            input.blur();
        } else if (e.key === 'Escape') {
            console.log("Close agent entirely");
        } else if (e.key === 'Enter') {
            console.log("Submit:", input.value);
        }
        return;
    }

    // If not expanded, and user types a printable character (ignore tab/esc/modifiers here)
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
        isExpanded = true;
        card.classList.add('is-expanded');
        
        setTimeout(() => {
            input.focus();
        }, 50); 
    } else if (e.key === 'Escape') {
        console.log("Cancelled recording");
    }
});

// Click to expand manually
card.addEventListener('click', () => {
    if (!isExpanded) {
        isExpanded = true;
        card.classList.add('is-expanded');
        setTimeout(() => {
            input.focus();
        }, 50);
    }
});

// Handle Send button click
sendBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    console.log("Submit:", input.value);
});

// Simulate "Scrap That"
const simScrapBtn = document.getElementById('sim-scrap');
const recordingIndicator = document.querySelector('.recording-indicator');

simScrapBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    
    // 1. Instantly collapse if expanded
    if (isExpanded) {
        isExpanded = false;
        card.classList.remove('is-expanded');
        input.value = '';
        input.blur();
    }
    
    // 2. Play the visual "wipe/reset" animation on the compact wave
    recordingIndicator.classList.remove('scrap-flash');
    // Force DOM reflow to restart animation
    void recordingIndicator.offsetWidth; 
    recordingIndicator.classList.add('scrap-flash');
});
