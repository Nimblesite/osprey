# Screenshot goldens [WINUI-HOST-GOLDENS]: compares each picture of the window's content a self-test
# took (<name>-content.png, drawn without the desktop behind the window) with the golden of the same
# name. A pixel differs when any channel is off by more than -Tolerance; a picture fails when more
# than -Fraction of its pixels differ or its size differs, and a diff picture is written beside it.
# A shot with no golden fails too. -Update replaces the goldens with these pictures.
#   goldens.ps1 -Shots <folder> -Goldens <folder> [-Update] [-Tolerance 24] [-Fraction 0.002]
param(
    [Parameter(Mandatory)] [string] $Shots,
    [Parameter(Mandatory)] [string] $Goldens,
    [switch] $Update,
    [int] $Tolerance = 24,
    [double] $Fraction = 0.002
)
$ErrorActionPreference = 'Stop'
$pictures = @(Get-ChildItem $Shots -Filter '*-content.png' | Sort-Object Name)
if ($pictures.Count -eq 0) { throw "no pictures of the window's content in $Shots" }
if ($Update) {
    New-Item -ItemType Directory -Force $Goldens | Out-Null
    Get-ChildItem $Goldens -Filter *.png | Remove-Item
    $pictures | ForEach-Object { Copy-Item $_.FullName (Join-Path $Goldens ($_.Name -replace '-content\.png$', '.png')) }
    "updated $($pictures.Count) goldens in $Goldens"
    return
}

Add-Type -ReferencedAssemblies System.Drawing.Common, System.Drawing.Primitives -TypeDefinition @'
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class GoldenCompare
{
    // The count of pixels that differ by more than tolerance in any channel, with each one painted
    // red in the diff and the rest faded; -1 when the sizes differ.
    public static long Differing(string shot, string golden, string diff, int tolerance)
    {
        using var a = new Bitmap(shot);
        using var b = new Bitmap(golden);
        if (a.Width != b.Width || a.Height != b.Height) return -1;
        var left = Pixels(a);
        var right = Pixels(b);
        var marked = new int[left.Length];
        long count = 0;
        for (var i = 0; i < left.Length; i++)
        {
            var off = Off(left[i], right[i], tolerance);
            count += off ? 1 : 0;
            marked[i] = off ? unchecked((int)0xFFFF0000) : (left[i] & 0x00FFFFFF) | 0x30000000;
        }
        if (count > 0)
        {
            using var picture = new Bitmap(a.Width, a.Height, PixelFormat.Format32bppArgb);
            var data = picture.LockBits(new Rectangle(0, 0, a.Width, a.Height), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
            Marshal.Copy(marked, 0, data.Scan0, marked.Length);
            picture.UnlockBits(data);
            picture.Save(diff, ImageFormat.Png);
        }
        return count;
    }

    static bool Off(int x, int y, int tolerance)
    {
        for (var shift = 0; shift < 32; shift += 8)
        {
            if (System.Math.Abs(((x >> shift) & 0xFF) - ((y >> shift) & 0xFF)) > tolerance) return true;
        }
        return false;
    }

    static int[] Pixels(Bitmap picture)
    {
        var data = picture.LockBits(new Rectangle(0, 0, picture.Width, picture.Height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        var pixels = new int[picture.Width * picture.Height];
        Marshal.Copy(data.Scan0, pixels, 0, pixels.Length);
        picture.UnlockBits(data);
        return pixels;
    }
}
'@

$failed = @()
foreach ($picture in $pictures) {
    $name = $picture.Name -replace '-content\.png$', ''
    $golden = Join-Path $Goldens "$name.png"
    if (-not (Test-Path $golden)) { $failed += "$name has no golden"; continue }
    $diff = Join-Path $Shots "$name-diff.png"
    $count = [GoldenCompare]::Differing($picture.FullName, $golden, $diff, $Tolerance)
    if ($count -lt 0) { $failed += "$name is not the golden's size"; continue }
    Add-Type -AssemblyName System.Drawing
    $image = [Drawing.Image]::FromFile($golden); $total = $image.Width * $image.Height; $image.Dispose()
    $share = $count / $total
    "{0}: {1} of {2} pixels differ ({3:P3})" -f $name, $count, $total, $share
    if ($share -gt $Fraction) { $failed += ("{0} differs from its golden in {1:P3} of its pixels; see {2}" -f $name, $share, $diff) }
}
if ($failed.Count -gt 0) { throw "Screenshot goldens failed:`n  $($failed -join "`n  ")" }
"PASS $($pictures.Count) goldens"
