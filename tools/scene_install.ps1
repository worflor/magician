<#
.SYNOPSIS
  Installs (or removes) a hero-page scene from a scene patch, using only Windows PowerShell. No Magician, no Python,
  nothing to download.

.DESCRIPTION
  A scene patch is a folder with scene-patch.txt and files/<path>: only the mod author's own files. The rest of the
  hero-page map comes from YOUR copy of Deadlock. This script:
    1. finds your game (or takes -Game),
    2. reads your stock maps\ui\hero_prefabs\<hero>.vpk,
    3. checks every Valve file the patch keeps or replaces against the checksum the author saw (a game update that
       changed the map stops here, and nothing is written),
    4. builds the scene VPK (Valve's files from your game plus the author's files) and writes it to
       citadel\addons\maps\ui\hero_prefabs\<hero>.vpk,
    5. checks the result is byte-for-byte the scene the author tested (the patch's `result` checksum).
  It is the same algorithm as `magician scene install` and writes the same bytes.

.PARAMETER Patch
  One scene-patch folder, or a folder of them (e.g. the bundle's `scene` folder). Default: `scene` beside this script.
.PARAMETER Game
  The Deadlock `game` folder (the one holding `citadel`). Default: $env:DEADLOCK_GAME, then Steam's library list.
  A folder given here or in DEADLOCK_GAME that is not Deadlock's is an error, never a reason to look elsewhere.
.PARAMETER Uninstall
  Remove the scene(s) this patch installs. Only a file whose checksum is the patch's result (or, for a patch without one,
  the scene the patch builds from your game) is removed (-Force overrides).
.PARAMETER Force
  Replace or remove a scene file that is not this patch's result (e.g. another mod's scene).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File install-scene.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File install-scene.ps1 -Game "D:\SteamLibrary\steamapps\common\Deadlock\game"
#>
param(
    [string]$Patch = (Join-Path $PSScriptRoot 'scene'),
    [string]$Game = $env:DEADLOCK_GAME,
    [switch]$Uninstall,
    [switch]$Force
)
$ErrorActionPreference = 'Stop'

Add-Type -Language CSharp -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

public static class SceneVpk {
    const uint Signature = 0x55AA1234;
    const ushort Inline = 0x7FFF, Terminator = 0xFFFF;
    static readonly uint[] Table = MakeTable();
    static uint[] MakeTable() {
        var t = new uint[256];
        for (uint i = 0; i < 256; i++) { uint c = i; for (int k = 0; k < 8; k++) c = (c & 1) != 0 ? 0xEDB88320u ^ (c >> 1) : c >> 1; t[i] = c; }
        return t;
    }
    public static uint Crc32(byte[] data) {
        uint c = 0xFFFFFFFFu;
        foreach (byte b in data) c = Table[(c ^ b) & 0xFF] ^ (c >> 8);
        return ~c;
    }

    public class Entry { public uint Crc; public byte[] Preload; public ushort Archive; public uint Offset, Length; }

    static string CStr(BinaryReader r) {
        var bytes = new List<byte>();
        byte b; while ((b = r.ReadByte()) != 0) bytes.Add(b);
        return Encoding.UTF8.GetString(bytes.ToArray());
    }

    // Reads a single-file VPK (v1 or v2) and returns every file's bytes, keyed by its path.
    public static Dictionary<string, byte[]> ReadAll(string path) {
        var all = File.ReadAllBytes(path);
        var r = new BinaryReader(new MemoryStream(all));
        if (r.ReadUInt32() != Signature) throw new Exception(path + " is not a VPK");
        uint version = r.ReadUInt32(); uint treeSize = r.ReadUInt32();
        int headerLen = version == 2 ? 28 : version == 1 ? 12 : -1;
        if (headerLen < 0) throw new Exception(path + ": VPK version " + version + " is not supported");
        r.BaseStream.Position = headerLen;
        long dataStart = headerLen + treeSize;
        var files = new Dictionary<string, byte[]>(StringComparer.Ordinal);
        while (true) {
            string ext = CStr(r); if (ext.Length == 0) break;
            while (true) {
                string dir = CStr(r); if (dir.Length == 0) break;
                while (true) {
                    string name = CStr(r); if (name.Length == 0) break;
                    var e = new Entry { Crc = r.ReadUInt32() };
                    ushort pre = r.ReadUInt16(); e.Archive = r.ReadUInt16(); e.Offset = r.ReadUInt32(); e.Length = r.ReadUInt32();
                    if (r.ReadUInt16() != Terminator) throw new Exception(path + ": damaged directory");
                    e.Preload = r.ReadBytes(pre);
                    if (e.Archive != Inline && e.Length > 0) throw new Exception(path + ": data in a separate archive is not supported");
                    if (e.Length > 0 && dataStart + e.Offset + e.Length > all.Length) throw new Exception(path + ": damaged directory (an entry runs past the end of the file)");
                    var bytes = new byte[pre + e.Length];
                    Buffer.BlockCopy(e.Preload, 0, bytes, 0, pre);
                    if (e.Length > 0) Buffer.BlockCopy(all, (int)(dataStart + e.Offset), bytes, pre, (int)e.Length);
                    string p = (dir == " " ? "" : dir + "/") + name + (ext == " " ? "" : "." + ext);
                    files[p] = bytes;
                }
            }
        }
        return files;
    }

    static void Split(string path, out string dir, out string name, out string ext) {
        int s = path.LastIndexOf('/');
        dir = s < 0 ? "" : path.Substring(0, s);
        string file = s < 0 ? path : path.Substring(s + 1);
        int d = file.LastIndexOf('.');
        name = d < 0 ? file : file.Substring(0, d);
        ext = d < 0 ? "" : file.Substring(d + 1);
        if (dir.Length == 0) dir = " "; if (name.Length == 0) name = " "; if (ext.Length == 0) ext = " ";
    }

    // Writes a v2 single-file VPK in Magician's deterministic layout (ext, dir, name in ordinal order; no preload).
    public static void Write(string outPath, IDictionary<string, byte[]> files) {
        var tree = new SortedDictionary<string, SortedDictionary<string, SortedDictionary<string, byte[]>>>(StringComparer.Ordinal);
        foreach (var kv in files) {
            string dir, name, ext; Split(kv.Key, out dir, out name, out ext);
            if (!tree.ContainsKey(ext)) tree[ext] = new SortedDictionary<string, SortedDictionary<string, byte[]>>(StringComparer.Ordinal);
            if (!tree[ext].ContainsKey(dir)) tree[ext][dir] = new SortedDictionary<string, byte[]>(StringComparer.Ordinal);
            tree[ext][dir][name] = kv.Value;
        }
        var t = new MemoryStream(); var tw = new BinaryWriter(t);
        var data = new MemoryStream();
        Action<string> cstr = s => { var b = Encoding.UTF8.GetBytes(s); tw.Write(b); tw.Write((byte)0); };
        foreach (var e in tree) {
            cstr(e.Key);
            foreach (var d in e.Value) {
                cstr(d.Key);
                foreach (var n in d.Value) {
                    cstr(n.Key);
                    tw.Write(Crc32(n.Value)); tw.Write((ushort)0); tw.Write(Inline);
                    tw.Write((uint)data.Length); tw.Write((uint)n.Value.Length); tw.Write(Terminator);
                    data.Write(n.Value, 0, n.Value.Length);
                }
                tw.Write((byte)0);
            }
            tw.Write((byte)0);
        }
        tw.Write((byte)0); tw.Flush();
        Directory.CreateDirectory(Path.GetDirectoryName(outPath));
        string tmp = outPath + ".tmp";
        using (var f = new BinaryWriter(File.Create(tmp))) {
            f.Write(Signature); f.Write(2u); f.Write((uint)t.Length); f.Write((uint)data.Length);
            f.Write(0u); f.Write(0u); f.Write(0u);
            t.WriteTo(f.BaseStream); data.WriteTo(f.BaseStream);
        }
        if (File.Exists(outPath)) File.Delete(outPath);
        File.Move(tmp, outPath);
    }
}
'@

function Hex([uint32]$v) { '{0:x8}' -f $v }

function Find-Game {
    param([string]$Given)
    if ($Given) {
        # the player named this folder: install there or nowhere, never into another game Steam knows of
        if (Test-Path -LiteralPath (Join-Path $Given 'citadel\gameinfo.gi')) { return (Resolve-Path -LiteralPath $Given).Path }
        throw "$Given is not Deadlock's ``game`` folder (no citadel\gameinfo.gi there). Point -Game (or DEADLOCK_GAME) at <...>\steamapps\common\Deadlock\game."
    }
    $cands = @()
    $steam = $null
    foreach ($k in 'HKCU:\Software\Valve\Steam', 'HKLM:\SOFTWARE\WOW6432Node\Valve\Steam') {
        try { $p = (Get-ItemProperty $k -ErrorAction Stop); if ($p.SteamPath) { $steam = $p.SteamPath } elseif ($p.InstallPath) { $steam = $p.InstallPath } } catch {}
        if ($steam) { break }
    }
    if ($steam) {
        $libs = @($steam)
        $vdf = Join-Path $steam 'steamapps\libraryfolders.vdf'
        if (Test-Path -LiteralPath $vdf) {
            foreach ($m in [regex]::Matches((Get-Content -LiteralPath $vdf -Raw), '"path"\s+"([^"]+)"')) { $libs += ($m.Groups[1].Value -replace '\\\\', '\') }
        }
        foreach ($l in $libs) { $cands += (Join-Path $l 'steamapps\common\Deadlock\game') }
    }
    foreach ($c in $cands) { if ($c -and (Test-Path -LiteralPath (Join-Path $c 'citadel\gameinfo.gi'))) { return (Resolve-Path -LiteralPath $c).Path } }
    throw "Deadlock not found. Run again with -Game ""<...>\steamapps\common\Deadlock\game"" (the folder that holds citadel)."
}

# Each line of scene-patch.txt and its field count (the name included); read as strictly as `magician scene` reads it.
$Fields = @{ 'map' = 2; 'mod' = 2; 'built_for' = 2; 'result' = 2; 'keep' = 4; 'replace' = 5; 'add' = 4 }

function Read-Patch([string]$Dir) {
    $txt = Join-Path $Dir 'scene-patch.txt'
    $p = @{ Dir = $Dir; Keep = @(); Replace = @(); Add = @() }
    $n = 0
    $lines = @(foreach ($line in Get-Content -LiteralPath $txt -Encoding UTF8) { $n++; if ($line -notmatch '^\s*(#|$)') { , @($n, $line) } })
    $head = if ($lines.Count) { $lines[0][1] } else { '' }
    if ($head -cne "magician-scene-patch`t1") {
        if ($head -clike "magician-scene-patch`t*") { throw "$txt`: patch format $(($head -split "`t")[1]) is newer than this script" }
        throw "$txt`: not a scene patch (it must start with magician-scene-patch<TAB>1)"
    }
    $hex8 = { param($v, $n) if ($v -cnotmatch '^[0-9a-fA-F]{8}$') { throw "$txt line $n`: '$v' is not a crc (8 hex digits)" }; $v.ToLowerInvariant() }
    $size = { param($v, $n) if ($v -cnotmatch '^[0-9]{1,18}$') { throw "$txt line $n`: size '$v' is not a number" }; [int64]$v }
    $seen = @{}
    foreach ($l in ($lines | Select-Object -Skip 1)) {
        $n = $l[0]
        $f = $l[1] -split "`t"
        $want = $Fields[$f[0]]
        if (-not $want -or -not ($Fields.Keys -ccontains $f[0])) { throw "$txt line $n`: unknown field '$($f[0])' (fields are tab-separated)" }
        if ($f.Count -ne $want) { throw "$txt line $n`: '$($f[0])' takes $($want - 1) field(s), found $($f.Count - 1)" }
        switch -CaseSensitive ($f[0]) {
            'map' { $p.Map = $f[1] }
            'mod' { $p.Mod = $f[1] }
            'built_for' { $p.Build = $f[1] }
            'result' { $p.Result = & $hex8 $f[1] $n }
            default {
                if ($seen.ContainsKey($f[1])) { throw "$txt line $n`: $($f[1]) is listed twice" }
                $seen[$f[1]] = $true
                switch -CaseSensitive ($f[0]) {
                    'keep' { $p.Keep += , @{ Path = $f[1]; Valve = (& $hex8 $f[2] $n); Size = (& $size $f[3] $n) } }
                    'replace' { $p.Replace += , @{ Path = $f[1]; Valve = (& $hex8 $f[2] $n); Ours = (& $hex8 $f[3] $n); Size = (& $size $f[4] $n) } }
                    'add' { $p.Add += , @{ Path = $f[1]; Ours = (& $hex8 $f[2] $n); Size = (& $size $f[3] $n) } }
                }
            }
        }
    }
    if (-not $p.Map -or $p.Map -cnotmatch '^maps/ui/hero_prefabs/[a-z0-9_]+\.vpk$') { throw "$txt`: unexpected map '$($p.Map)'" }
    if (-not $p.Mod) { throw "$txt`: the patch names no mod" }
    $hero = [IO.Path]::GetFileNameWithoutExtension($p.Map)
    foreach ($e in $p.Keep + $p.Replace + $p.Add) {
        $badSeg = @($e.Path -split '/' | Where-Object { $_ -eq '' -or $_ -eq '.' -or $_ -eq '..' }).Count
        if ($e.Path -cnotmatch "^maps/ui/hero_prefabs/$hero(\.vmap_c|/[a-z0-9_./-]+)$" -or $badSeg) { throw "$txt`: refusing path '$($e.Path)'" }
    }
    if (-not $seen.ContainsKey("maps/ui/hero_prefabs/$hero.vmap_c")) { throw "$txt`: the patch has no maps/ui/hero_prefabs/$hero.vmap_c; not a scene for $hero" }
    $p
}

# Every file of the scene, from the player's stock map and the patch, after checking each against the patch; throws,
# listing what differs, when the map is not the one the patch was made against.
function Get-SceneFiles($p, [string]$GameDir) {
    $citadel = Join-Path $GameDir 'citadel'
    $stock = Join-Path $citadel ($p.Map -replace '/', '\')
    if (-not (Test-Path -LiteralPath $stock)) { throw "Your game has no $($p.Map). Is this the right Deadlock folder?" }
    $valve = [SceneVpk]::ReadAll($stock)
    $problems = @()
    $out = New-Object 'System.Collections.Generic.Dictionary[string,byte[]]' ([StringComparer]::Ordinal)
    foreach ($e in $p.Keep + $p.Replace) {
        if (-not $valve.ContainsKey($e.Path)) { $problems += "missing in your game: $($e.Path)"; continue }
        $crc = Hex ([SceneVpk]::Crc32($valve[$e.Path]))
        if ($crc -ne $e.Valve) { $problems += "changed in your game: $($e.Path) ($crc, the patch expects $($e.Valve))" }
        elseif ($p.Keep -contains $e -and $valve[$e.Path].Length -ne $e.Size) { $problems += "changed in your game: $($e.Path) (size $($valve[$e.Path].Length), the patch expects $($e.Size))" }
    }
    foreach ($e in $p.Add) {
        if ($valve.ContainsKey($e.Path)) { $problems += "new in your game: $($e.Path) (the patch adds it; Valve's map now has a file there)" }
    }
    foreach ($e in $p.Keep) { if ($valve.ContainsKey($e.Path)) { $out[$e.Path] = $valve[$e.Path] } }
    foreach ($e in $p.Replace + $p.Add) {
        $f = Join-Path $p.Dir ('files\' + ($e.Path -replace '/', '\'))
        if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { $problems += "missing from the patch: $($e.Path)"; continue }
        $b = [IO.File]::ReadAllBytes($f)
        if ((Hex ([SceneVpk]::Crc32($b))) -ne $e.Ours -or $b.Length -ne $e.Size) { $problems += "damaged in the patch: $($e.Path)"; continue }
        $out[$e.Path] = $b
    }
    if ($problems.Count) {
        $problems | ForEach-Object { Write-Host "  $_" }
        throw "This patch was made for build $($p.Build) and your game's $($p.Map) no longer matches it. Nothing was written. A game update changed Valve's map, so this patch needs a new release from its author."
    }
    , $out
}

function Install-One($p, [string]$GameDir) {
    $citadel = Join-Path $GameDir 'citadel'
    $target = Join-Path $citadel ('addons\' + ($p.Map -replace '/', '\'))
    $out = Get-SceneFiles $p $GameDir
    # Built beside the target and renamed into place: never half a scene file in addons, never a temp file left behind.
    New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
    $tmpOut = "$target.partial"
    try {
        [SceneVpk]::Write($tmpOut, $out)
        $built = [IO.File]::ReadAllBytes($tmpOut)
        $builtCrc = Hex ([SceneVpk]::Crc32($built))
        if ($p.Result -and $builtCrc -ne $p.Result) {
            throw "The built scene ($builtCrc) is not the one the author tested ($($p.Result)). Nothing was written."
        }
        if (Test-Path -LiteralPath $target) {
            $have = Hex ([SceneVpk]::Crc32([IO.File]::ReadAllBytes($target)))
            if ($have -eq $builtCrc) { Write-Host "  already installed ($builtCrc)"; return }
            if (-not $Force) { throw "$target already exists and is not this scene ($have). Run with -Force to replace it." }
        }
        Move-Item -LiteralPath $tmpOut -Destination $target -Force
    } finally {
        if (Test-Path -LiteralPath $tmpOut) { Remove-Item -LiteralPath $tmpOut -Force }
    }
    Write-Host "  installed $($p.Map) ($builtCrc) for $($p.Mod)"
    $gi = Get-Content -LiteralPath (Join-Path $citadel 'gameinfo.gi') -Raw
    if ($gi -notmatch 'citadel/addons') {
        Write-Host "  note: gameinfo.gi has no citadel/addons search path, so the game will not load addons yet. A mod manager (or installing the skin) adds it."
    }
}

function Uninstall-One($p, [string]$GameDir) {
    $target = Join-Path $GameDir ('citadel\addons\' + ($p.Map -replace '/', '\'))
    if (-not (Test-Path -LiteralPath $target)) { Write-Host "  not installed"; return }
    $have = Hex ([SceneVpk]::Crc32([IO.File]::ReadAllBytes($target)))
    $expected = $p.Result
    if (-not $expected -and -not $Force) {
        # No result line: the scene this patch makes is its own answer. Build it (in a temporary file) and compare.
        try { $out = Get-SceneFiles $p $GameDir } catch {
            throw "This patch records no result checksum and your game's $($p.Map) no longer matches it, so there is no telling whether $target is this patch's scene. Run with -Force to remove it anyway."
        }
        $tmp = Join-Path ([IO.Path]::GetTempPath()) ("magician-scene-" + [Guid]::NewGuid().ToString('N') + ".vpk")
        try {
            [SceneVpk]::Write($tmp, $out)
            $expected = Hex ([SceneVpk]::Crc32([IO.File]::ReadAllBytes($tmp)))
        } finally {
            if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force }
        }
    }
    if ($expected -and $have -ne $expected -and -not $Force) { throw "$target is not this patch's scene ($have, expected $expected). Run with -Force to remove it anyway." }
    Remove-Item -LiteralPath $target
    Write-Host "  removed $($p.Map)"
}

$patches = if (Test-Path -LiteralPath (Join-Path $Patch 'scene-patch.txt')) { @($Patch) } elseif (Test-Path -LiteralPath $Patch -PathType Container) { @(Get-ChildItem -LiteralPath $Patch -Directory | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'scene-patch.txt') } | ForEach-Object FullName) } else { @() }
if (-not $patches.Count) { Write-Host "No scene patch found in $Patch"; exit 2 }
try { $gameDir = Find-Game $Game } catch { Write-Host $_.Exception.Message; exit 2 }
Write-Host "Deadlock: $gameDir"
$failed = $false
foreach ($dir in $patches) {
    Write-Host ""
    Write-Host "== $(Split-Path $dir -Leaf)"
    try {
        $p = Read-Patch $dir
        if ($Uninstall) { Uninstall-One $p $gameDir } else { Install-One $p $gameDir }
    } catch { Write-Host "  $($_.Exception.Message)"; $failed = $true }
}
Write-Host ""
if ($failed) { Write-Host "Something above failed. Nothing was left half-done."; exit 1 } else { Write-Host "Done." }
