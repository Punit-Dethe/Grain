// ═══ GRAIN-01 control surface ════════════════════════════════════════════════

// ─── clock ───
const clock = document.getElementById('clock')
setInterval(() => {
  clock.textContent = new Date().toLocaleTimeString('en-GB', { hour12: false })
}, 1000)

// ─── ticker: duplicate row for seamless loop ───
const tk = document.getElementById('tickerRow')
tk.innerHTML += tk.innerHTML

// ─── reveal on scroll ───
const modC = document.querySelector('.rack .mod.rv-d2')   // module C — last to animate in
const io = new IntersectionObserver(es => {
  es.forEach(e => {
    if (!e.isIntersecting) return
    e.target.classList.add('in')
    io.unobserve(e.target)
    // after module C finishes its animation (delay 0.24s + transition 0.7s), show cables
    if (e.target === modC) setTimeout(revealCables, 940)
  })
}, { threshold: 0.18 })
document.querySelectorAll('.rv, .panel').forEach(el => io.observe(el))

function revealCables() {} // cables removed — kept stub so IO observer doesn't error

// ─── 02 latency bench (runs when scrolled into view, loops) ───
const bench = document.getElementById('bench')
const aSpeak = document.getElementById('aSpeak')
const aWait = document.getElementById('aWait')
const bLive = document.getElementById('bLive')
const bPaste = document.getElementById('bPaste')
const timeA = document.getElementById('timeA')
const timeB = document.getElementById('timeB')

function fmt(s) {
  return `+${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')} MIN`
}

function runBench() {
  // compressed timeline: 5 min of speech. grain pastes on release (+0:00);
  // record-first tools transcribe for ~2 more minutes.
  const SPEAK = 2400, WAIT = 1100, HOLD = 1600
  const T0 = performance.now()
  aSpeak.style.width = aWait.style.width = bLive.style.width = '0%'
  bPaste.classList.remove('show')

  function step(now) {
    const t = now - T0
    if (t < SPEAK) {
      const p = t / SPEAK
      aSpeak.style.width = p * 60 + '%'
      bLive.style.width = p * 60 + '%'
      timeA.textContent = timeB.textContent = fmt(p * 300)
      requestAnimationFrame(step)
    } else if (t < SPEAK + WAIT) {
      if (!bPaste.classList.contains('show')) {
        bPaste.classList.add('show')
        timeB.textContent = fmt(300) + ' · PASTED'
      }
      const p = (t - SPEAK) / WAIT
      aWait.style.width = p * 36 + '%'
      timeA.textContent = fmt(300 + p * 120) + ' · STILL WAITING'
      requestAnimationFrame(step)
    } else if (t < SPEAK + WAIT + HOLD) {
      timeA.textContent = fmt(420) + ' · DONE'
      requestAnimationFrame(step)
    } else {
      runBench() // loop
    }
  }
  requestAnimationFrame(step)
}

const benchIO = new IntersectionObserver(es => {
  es.forEach(e => { if (e.isIntersecting) { runBench(); benchIO.unobserve(e.target) } })
}, { threshold: 0.4 })
benchIO.observe(bench)

// ─── 03 unload knob ───
const knob = document.getElementById('knob')
const knobOpts = document.querySelectorAll('.knob-opts span')
const knobAngles = [-135, -45, 45, 135]
let kIdx = 1
function setKnob(i) {
  kIdx = i
  knob.style.setProperty('--rot', knobAngles[i] + 'deg')
  knobOpts.forEach((o, j) => o.classList.toggle('on', j === i))
}
knob.addEventListener('click', () => setKnob((kIdx + 1) % 4))
knob.addEventListener('keydown', e => {
  if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setKnob((kIdx + 1) % 4) }
})
knobOpts.forEach(o => o.addEventListener('click', () => setKnob(+o.dataset.k)))
setKnob(1)

// ─── 04 routing LEDs + decision surface ───
const rows = document.querySelectorAll('[data-rt]')
const routeState = document.getElementById('routeState')
const gQ = document.getElementById('gQ'), gQv = document.getElementById('gQv')
const gL = document.getElementById('gL'), gLv = document.getElementById('gLv')
const gC = document.getElementById('gC'), gCv = document.getElementById('gCv')
const states = ['QUOTA CHECK → OK', 'ROUTED', 'CONTEXT FIT → OK', 'FALLBACK ARMED', 'KEY ROTATED']
let ri = 0
function routeTick() {
  rows[ri].classList.remove('live')
  ri = (ri + 1) % rows.length
  rows[ri].classList.add('live')
  routeState.textContent = states[Math.floor(Math.random() * states.length)]
  const q = 38 + Math.random() * 58, l = 30 + Math.random() * 120, c = 55 + Math.random() * 45
  gQ.style.width = q + '%';        gQv.textContent = Math.round(q) + '% LEFT'
  gL.style.width = (130 - l) / 1.3 + '%'; gLv.textContent = Math.round(l) + ' MS'
  gC.style.width = c + '%';        gCv.textContent = c > 95 ? 'FULL' : 'OK'
}
if (rows.length) {
  rows[0].classList.add('live')
  routeTick()
  setInterval(routeTick, 1700)
}

// ─── HERO FIELD — signal becoming language ───
const fcv = document.getElementById('fieldCv')
const fc = fcv.getContext('2d')
const GLYPHS = 'GRAIN10▌·—abcdefghijklmnopqrstuvwxyz'
let fw = 0, fh = 0, ft = 0
let fmx = -9999, fmy = -9999, fEnergy = 0

function sizeField() {
  fw = fcv.offsetWidth; fh = fcv.offsetHeight
  fcv.width = fw * devicePixelRatio
  fcv.height = fh * devicePixelRatio
}
new ResizeObserver(sizeField).observe(fcv)
sizeField()

// listen on the whole hero — mousemove bubbles up from the text and
// buttons too, so the field stays alive under every element
const fieldEl = document.querySelector('.hero')
fieldEl.addEventListener('mousemove', e => {
  const r = fieldEl.getBoundingClientRect()
  fmx = e.clientX - r.left; fmy = e.clientY - r.top
  fEnergy = Math.min(fEnergy + 0.09, 1.6)
})
fieldEl.addEventListener('mouseleave', () => { fmx = fmy = -9999 })

const P = []
const NP = 520
function seedParticle(p, fresh) {
  p.x = fresh ? Math.random() * fw : -10 - Math.random() * 60
  p.y = fh / 2 + (Math.random() - 0.5) * fh * 0.5
  p.v = 1.1 + Math.random() * 1.7
  p.ph = Math.random() * Math.PI * 2
  p.fr = 0.4 + Math.random() * 1.4          // wave frequency
  p.g = GLYPHS[Math.floor(Math.random() * GLYPHS.length)]
  p.sz = 9 + Math.random() * 4
  return p
}
for (let i = 0; i < NP; i++) P.push(seedParticle({}, true))

function drawField() {
  ft += 0.016
  fEnergy = Math.max(fEnergy * 0.985, 0.18)
  fc.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0)
  // trail fade — beige paper
  fc.fillStyle = 'rgba(222,213,197,0.26)'
  fc.fillRect(0, 0, fw, fh)

  const third = fw / 3
  fc.textBaseline = 'middle'

  for (const p of P) {
    p.x += p.v * (1 + fEnergy * 0.7)
    const stage = p.x / fw   // 0..1 across the section

    // base wave path — amplitude swells with cursor energy and proximity
    const dx = p.x - fmx
    const prox = Math.max(0, 1 - Math.abs(dx) / 260)
    const amp = (fh * 0.10 + fEnergy * fh * 0.16) * (1 + prox * 1.8)

    if (stage < 0.34) {
      // I. raw signal — particles ride a chaotic waveform
      const w = Math.sin(ft * 2.2 + p.x * 0.02 * p.fr + p.ph) * 0.6
            + Math.sin(ft * 3.7 + p.x * 0.045 + p.ph * 2) * 0.4
      p.y += ((fh / 2 + w * amp) - p.y) * 0.16
      fc.fillStyle = `rgba(255,93,30,${0.35 + prox * 0.55})`
      fc.beginPath()
      fc.arc(p.x, p.y, 1.6 + prox * 1.6, 0, Math.PI * 2)
      fc.fill()
    } else if (stage < 0.62) {
      // II. processing — the signal shatters into turbulence
      const n = Math.sin(p.y * 0.05 + ft * 3 + p.ph) * Math.cos(p.x * 0.03 - ft * 2)
      p.y += n * (2.4 + fEnergy * 3.5) + (fmy > 0 ? (fmy - p.y) * prox * 0.02 : 0)
      const a = 0.25 + Math.abs(n) * 0.5
      fc.fillStyle = Math.random() < 0.08 ? `rgba(22,18,13,${a})` : `rgba(255,93,30,${a})`
      fc.fillRect(p.x - 1.2, p.y - 1.2, 2.4, 2.4)
    } else {
      // III. language — particles snap to a typographic grid and become glyphs
      const row = Math.round((p.y - fh / 2) / 26) * 26 + fh / 2
      p.y += (row - p.y) * 0.2
      const settle = Math.min(1, (stage - 0.62) / 0.2)
      if (Math.random() < 0.025) p.g = GLYPHS[Math.floor(Math.random() * GLYPHS.length)]
      fc.font = `${p.sz}px JetBrains Mono, monospace`
      const inked = settle > 0.7
      fc.fillStyle = inked
        ? `rgba(22,18,13,${0.18 + settle * 0.55})`   // settled: ink on paper
        : `rgba(255,93,30,${0.3 + settle * 0.5})`
      fc.fillText(p.g, Math.round(p.x / 13) * 13, row)
    }

    if (p.x > fw + 20) seedParticle(p, false)
  }


  requestAnimationFrame(drawField)
}
drawField()

// ─── 05 agent action cycle ───
const agentOut = document.getElementById('agentOut')
const actions = ['SUMMARIZE', 'RESTRUCTURE', 'DRAFT EMAIL', 'ASK ANYTHING']
let ai = 0
setInterval(() => {
  ai = (ai + 1) % actions.length
  agentOut.textContent = actions[ai]
}, 2200)
