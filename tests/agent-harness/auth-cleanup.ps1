# Remove only the exact native-auth fixture entries for one owned run.
# Never reads or prints a credential blob; no ordinary namespace is accepted.
param([Parameter(Mandatory=$true)][string]$Root, [Parameter(Mandatory=$true)][Guid]$RunId, [switch]$InventoryOnly)
$ErrorActionPreference = 'Stop'
$taskRoot = (Resolve-Path -LiteralPath $Root).Path
$taskMarker = Get-Content -Raw -LiteralPath (Join-Path $taskRoot '.grain-agent-harness.json') | ConvertFrom-Json
if ($taskMarker.schema -ne 1 -or [Guid]$taskMarker.runId -ne $RunId -or -not $taskMarker.authPort) {
    throw 'Native vault cleanup requires the exact enabled run marker'
}
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;
public static class GrainOwnedAuthCleanup {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    private struct Credential {
        public uint Flags, Type;
        public IntPtr TargetName, Comment;
        public long LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist, AttributeCount;
        public IntPtr Attributes, TargetAlias, UserName;
    }
    [DllImport("advapi32.dll", EntryPoint="CredEnumerateW", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern bool Enumerate(string filter, uint flags, out uint count, out IntPtr entries);
    [DllImport("advapi32.dll", EntryPoint="CredDeleteW", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern bool Delete(string target, uint type, uint flags);
    [DllImport("advapi32.dll")] private static extern void CredFree(IntPtr pointer);
    public static int Clean(string runId, bool delete) {
        Guid parsed;
        if (!Guid.TryParseExact(runId, "D", out parsed)) throw new Exception("Invalid run UUID");
        string service = "com.grain.extension.oauth.agent-harness." + parsed.ToString("D");
        string suffix = "." + service;
        string pattern = "^com\\.grain\\.harness\\.auth(?:-peer)?(?:/[a-fA-F0-9]{32})?" + Regex.Escape(suffix) + "$";
        uint count; IntPtr entries;
        if (!Enumerate("com.grain.harness.auth*", 0, out count, out entries)) {
            int error = Marshal.GetLastWin32Error();
            if (error == 1168) return 0;
            throw new Win32Exception(error);
        }
        try {
            if (count > 256) throw new Exception("Fixture credential metadata bound exceeded");
            int matched = 0;
            for (int i=0; i<count; i++) {
                Credential credential = Marshal.PtrToStructure<Credential>(Marshal.ReadIntPtr(entries, i*IntPtr.Size));
                string target = Marshal.PtrToStringUni(credential.TargetName);
                if (target == null || !target.EndsWith(suffix, StringComparison.Ordinal)) continue;
                if (credential.Type != 1 || !Regex.IsMatch(target, pattern)) throw new Exception("Unexpected owned credential key; refused cleanup");
                matched++;
                if (delete && !Delete(target, credential.Type, 0)) throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            return matched;
        } finally { CredFree(entries); }
    }
}
'@
if ($InventoryOnly) {
    @{schema=1; kind='native-vault-inventory'; count=[GrainOwnedAuthCleanup]::Clean($RunId.ToString('D'), $false)} | ConvertTo-Json -Compress
    exit 0
}
$taskDeleted = [GrainOwnedAuthCleanup]::Clean($RunId.ToString('D'), $true)
$taskRemaining = [GrainOwnedAuthCleanup]::Clean($RunId.ToString('D'), $false)
if ($taskRemaining -ne 0) { throw 'Owned native credentials remain after cleanup' }
@{schema=1; kind='native-vault-cleanup'; deleted=$taskDeleted; remaining=$taskRemaining} | ConvertTo-Json -Compress
