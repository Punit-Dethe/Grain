# Send one real Windows Escape only after confirming the owned Agent is focused.
param([Parameter(Mandatory = $true)][int]$OwnerPid, [switch]$ProbeOnly, [switch]$FocusByClick)
$ErrorActionPreference = 'Stop'
if ($OwnerPid -le 0) { throw 'An owned host PID is required' }
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class HarnessInput {
    public static bool ForegroundRequested, ForegroundBefore, ForegroundAfter;
    public static uint Accepted, FocusClickAccepted;
    public static string HeldModifiers = "";
    public static bool EscapeHeld, PhysicalCoordinates;
    private delegate bool EnumProc(IntPtr hwnd, IntPtr unused);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr unused);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", SetLastError=true)] private static extern uint SendInput(uint count, Input[] inputs, int size);
    [StructLayout(LayoutKind.Sequential)] private struct Keyboard { public ushort vk, scan; public uint flags, time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] private struct Mouse { public int x, y; public uint data, flags, time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] private struct Point { public int x, y; }
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Explicit, Size=32)] private struct InputUnion { [FieldOffset(0)] public Keyboard keyboard; [FieldOffset(0)] public Mouse mouse; }
    [StructLayout(LayoutKind.Sequential)] private struct Input { public uint type; public InputUnion data; }
    public static string ModifierProbe() {
        var pressed = new System.Collections.Generic.List<int>();
        foreach (int key in new[] {16,17,18,91,92})
            if ((GetAsyncKeyState(key) & 0x8000) != 0) pressed.Add(key);
        return string.Join(",", pressed);
    }
    private static void RequireIdle() {
        HeldModifiers = ModifierProbe();
        EscapeHeld = (GetAsyncKeyState(0x1B) & 0x8000) != 0;
        if (HeldModifiers.Length != 0 || EscapeHeld || (GetAsyncKeyState(1) & 0x8000) != 0)
            throw new InvalidOperationException("Refusing input: key or mouse button held; Escape requires an idle desktop");
    }
    private static bool StillOwned(IntPtr target, uint owner) {
        uint pid; GetWindowThreadProcessId(target, out pid);
        if (pid!=owner || !IsWindowVisible(target)) return false;
        var title = new StringBuilder(256); GetWindowText(target, title, title.Capacity);
        return title.ToString()=="Grain Assist";
    }
    private static void FocusByOwnedClick(IntPtr target, uint owner) {
        var previous = SetThreadDpiAwarenessContext(new IntPtr(-4)); // PER_MONITOR_AWARE_V2
        if (previous==IntPtr.Zero) throw new InvalidOperationException("Owned click requires per-monitor physical coordinates");
        PhysicalCoordinates = true;
        try { FocusByOwnedClickPhysical(target, owner); }
        finally { SetThreadDpiAwarenessContext(previous); }
    }
    private static void FocusByOwnedClickPhysical(IntPtr target, uint owner) {
        // Actual input focuses the owned surface, without AttachThreadInput or
        // a privileged application focus hook. Never click through an obstruction.
        Rect rect;
        if (!GetWindowRect(target, out rect) || rect.right-rect.left<80 || rect.bottom-rect.top<40)
            throw new InvalidOperationException("Refusing input: owned Agent header unavailable");
        var header = new Point { x=rect.left+32, y=rect.top+18 };
        if (!StillOwned(target, owner) || GetAncestor(WindowFromPoint(header), 2)!=target)
            throw new InvalidOperationException("Refusing input: owned Agent header obstructed");
        RequireIdle();
        int left=GetSystemMetrics(76), top=GetSystemMetrics(77), width=GetSystemMetrics(78), height=GetSystemMetrics(79);
        if (width<2 || height<2 || header.x<left || header.x>=left+width || header.y<top || header.y>=top+height)
            throw new InvalidOperationException("Refusing input: owned header is outside the virtual desktop");
        int x=(int)((long)(header.x-left)*65535/(width-1));
        int y=(int)((long)(header.y-top)*65535/(height-1));
        RequireIdle();
        if (!StillOwned(target, owner) || GetAncestor(WindowFromPoint(header), 2)!=target)
            throw new InvalidOperationException("Refusing input: owned focus target changed");
        var click = new[] {
            // Absolute virtual-desktop coordinates bind both queued events to
            // this header instead of relying on the cursor's later position.
            new Input { type=0, data=new InputUnion { mouse=new Mouse { x=x, y=y, flags=0xC003 } } },
            new Input { type=0, data=new InputUnion { mouse=new Mouse { x=x, y=y, flags=0xC005 } } }
        };
        FocusClickAccepted = SendInput(2, click, Marshal.SizeOf(typeof(Input)));
        // If only our down event was inserted, release only that synthetic hold.
        // This remains a failure; it is cleanup, not another focus attempt.
        if (FocusClickAccepted==1) SendInput(1, new[] { click[1] }, Marshal.SizeOf(typeof(Input)));
        if (FocusClickAccepted!=2) throw new InvalidOperationException("Windows did not accept the owned focus click");
        // Leave the pointer here; immediate restoration could move the queued
        // click to another application before Windows consumes it.
    }
    public static void Escape(uint owner, bool focusByClick) {
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
        ForegroundRequested = SetForegroundWindow(target);
        for (int attempt=0; attempt<20 && GetForegroundWindow()!=target; attempt++) Thread.Sleep(25);
        if (focusByClick || GetForegroundWindow()!=target) {
            FocusByOwnedClick(target, owner);
            for (int attempt=0; attempt<20 && GetForegroundWindow()!=target; attempt++) Thread.Sleep(25);
        }
        uint focused; GetWindowThreadProcessId(GetForegroundWindow(), out focused);
        ForegroundBefore = GetForegroundWindow()==target && focused==owner && StillOwned(target, owner);
        if (!ForegroundBefore) throw new InvalidOperationException("Refusing input: owned Agent is not foreground");
        RequireIdle();
        var inputs = new[] {
            new Input { type=1, data=new InputUnion { keyboard=new Keyboard { vk=0x1B } } },
            new Input { type=1, data=new InputUnion { keyboard=new Keyboard { vk=0x1B, flags=2 } } }
        };
        Accepted = SendInput(2, inputs, Marshal.SizeOf(typeof(Input)));
        if (Accepted==1) SendInput(1, new[] { inputs[1] }, Marshal.SizeOf(typeof(Input)));
        ForegroundAfter = GetForegroundWindow()==target;
        if (Accepted!=2) throw new InvalidOperationException("Windows did not accept the Escape input pair");
    }
}
'@
if ($ProbeOnly) {
    [Console]::WriteLine('Held modifier virtual keys: ' + [HarnessInput]::ModifierProbe())
} else {
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    $inputFailure = $null
    try { [HarnessInput]::Escape([uint32]$OwnerPid, [bool]$FocusByClick) }
    catch { $inputFailure = $_ }
    finally {
        # Coarse owned-window facts only: no foreign titles, PIDs or typed text.
        [ordered]@{
            schema = 1
            kind = 'native-escape'
            foregroundRequested = [HarnessInput]::ForegroundRequested
            foregroundBefore = [HarnessInput]::ForegroundBefore
            foregroundAfter = [HarnessInput]::ForegroundAfter
            heldModifiers = [HarnessInput]::HeldModifiers
            escapeHeld = [HarnessInput]::EscapeHeld
            accepted = [HarnessInput]::Accepted
            focusClickAccepted = [HarnessInput]::FocusClickAccepted
            physicalCoordinates = [HarnessInput]::PhysicalCoordinates
            elapsedMs = $timer.ElapsedMilliseconds
        } | ConvertTo-Json -Compress | Write-Output
    }
    if ($null -ne $inputFailure) { throw $inputFailure }
}
