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
    
    // Dynamic Orange to White Gradient
    // Orange on left: rgb(255, 85, 0)
    // White on right: rgb(255, 255, 255)
    const ratio = c / (WAVE_COLS - 1); // 0 at left, 1 at right
    const g = Math.round(85 + (170 * ratio)); // 85 to 255
    const b = Math.round(0 + (255 * ratio));  // 0 to 255
    dot.style.backgroundColor = `rgb(255, ${g}, ${b})`;

    if (isCorner) {
        dot.style.visibility = 'hidden';
    }
    waveGrid.appendChild(dot);
    dots.push(dot);
}

// Start Wave Animation
const start = performance.now();
function tick(now) {
    const angle = (((now - start) % 2000) / 2000) * Math.PI * 2;
    for (let i = 0; i < dots.length; i++) {
        const c = i % WAVE_COLS;
        const r = Math.floor(i / WAVE_COLS);
        const isCorner = (r === 0 || r === WAVE_ROWS - 1) && (c === 0 || c === WAVE_COLS - 1);
        if (!isCorner) {
            dots[i].style.opacity = Math.max(0, Math.sin(angle * 2.1 + c * 1.37 + r * 3.11));
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
