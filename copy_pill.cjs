const fs = require('fs');
fs.mkdirSync('src-tauri/binaries', { recursive: true });
const paths = ['target/release/grain-pill.exe', 'C:/gt/release/grain-pill.exe'];
let newestPath = null;
let maxMtime = 0;
for (const p of paths) {
    if (fs.existsSync(p)) {
        const stats = fs.statSync(p);
        if (stats.mtimeMs > maxMtime) {
            maxMtime = stats.mtimeMs;
            newestPath = p;
        }
    }
}
if (newestPath) {
    console.log(`Copying grain-pill.exe from ${newestPath}`);
    fs.copyFileSync(newestPath, 'src-tauri/binaries/grain-pill-x86_64-pc-windows-msvc.exe');
} else {
    console.error("Could not find grain-pill.exe!");
    process.exit(1);
}
