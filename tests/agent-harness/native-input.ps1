# Send one real Windows Escape only after confirming the owned Agent is focused.
param([Parameter(Mandatory = $true)][int]$OwnerPid, [switch]$ProbeOnly)
$ErrorActionPreference = 'Stop'
if ($OwnerPid -le 0) { throw 'An owned host PID is required' }
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class HarnessInput {
    private delegate bool EnumProc(IntPtr hwnd, IntPtr unused);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr unused);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll", SetLastError=true)] private static extern uint SendInput(uint count, Input[] inputs, int size);
    [StructLayout(LayoutKind.Sequential)] private struct Keyboard { public ushort vk, scan; public uint flags, time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Explicit, Size=32)] private struct InputUnion { [FieldOffset(0)] public Keyboard keyboard; }
    [StructLayout(LayoutKind.Sequential)] private struct Input { public uint type; public InputUnion data; }
    public static string ModifierProbe() {
        var pressed = new System.Collections.Generic.List<int>();
        foreach (int key in new[] {16,17,18,91,92})
            if ((GetAsyncKeyState(key) & 0x8000) != 0) pressed.Add(key);
        return string.Join(",", pressed);
    }
    public static void Escape(uint owner) {
        if (IntPtr.Size != 8) throw new InvalidOperationException("Native input adapter requires 64-bit Windows PowerShell");
        IntPtr target = IntPtr.Zero;
        int matches = 0;
        EnumWindows((hwnd, unused) => {
            uint pid; GetWindowThreadProcessId(hwnd, out pid);
            if (pid != owner || !IsWindowVisible(hwnd)) return true;
            var title = new StringBuilder(256); GetWindowText(hwnd, title, title.Capacity);
            if (title.ToString() != "Grain Assist") return true;
            matches++; target = hwnd; return true;
        }, IntPtr.Zero);
        if (matches > 1) throw new InvalidOperationException("Multiple owned Agent windows");
        if (target == IntPtr.Zero) throw new InvalidOperationException("Owned visible Agent window missing");
        SetForegroundWindow(target);
        for (int attempt=0; attempt<20 && GetForegroundWindow()!=target; attempt++) Thread.Sleep(25);
        uint focused; GetWindowThreadProcessId(GetForegroundWindow(), out focused);
        if (GetForegroundWindow()!=target || focused!=owner) throw new InvalidOperationException("Refusing input: owned Agent is not foreground");
        if (ModifierProbe().Length != 0) throw new InvalidOperationException("Refusing input: modifier key held; plain Escape requires an idle keyboard");
        var inputs = new[] {
            new Input { type=1, data=new InputUnion { keyboard=new Keyboard { vk=0x1B } } },
            new Input { type=1, data=new InputUnion { keyboard=new Keyboard { vk=0x1B, flags=2 } } }
        };
        if (SendInput(2, inputs, Marshal.SizeOf(typeof(Input)))!=2) throw new InvalidOperationException("Windows did not accept the Escape input pair");
    }
}
'@
if ($ProbeOnly) {
    [Console]::WriteLine('Held modifier virtual keys: ' + [HarnessInput]::ModifierProbe())
} else {
    [HarnessInput]::Escape([uint32]$OwnerPid)
}
