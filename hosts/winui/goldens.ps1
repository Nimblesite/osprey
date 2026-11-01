# Screenshot goldens [WINUI-HOST-GOLDENS]: compares each picture of the window's content a self-test
# took (<name>-content.png, drawn without the desktop behind the window) with the golden of the same
# name. A pixel differs when any channel is off by more than -Tolerance; a picture fails when more
# than -Fraction of its pixels differ or its size differs, and a diff picture is written beside it.
# A shot with no golden fails too. -Update replaces the goldens with these pictures.
# A self-test on screen also keeps <name>-screen.png, the window's client area as the screen shows it;
# it is held to the same golden over the golden's opaque pixels, since where the content is clear the
# screen shows the Mica backdrop, which follows the wallpaper, lined up within a few pixels, since the
# content's picture can reach a little past the area it is laid out in. So what a person sees is checked too.
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

Add-Type -ReferencedAssemblies System.Drawing.Common, System.Drawing.Primitives, System.Private.Windows.GdiPlus, System.Private.Windows.Core -TypeDefinition @'
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class GoldenCompare
{
    // The count of pixels that differ by more than tolerance in any channel, with each one painted
    // red in the diff and the rest faded; -1 when the sizes differ.
    // The pixels of a picture of the screen that differ from the golden's opaque pixels where the two
    // overlap, at the offset of at most Slack pixels each way that lines them up best; the screen's
    // content area can sit a few pixels inside the content's own picture. The opaque pixels compared
    // come back through total, and -1 when the sizes are further apart than the slack.
    public static long DifferingOnScreen(string shot, string golden, string diff, int tolerance, out long total)
    {
        const int Slack = 8;
        using var a = new Bitmap(shot);
        using var b = new Bitmap(golden);
        total = 0;
        if (System.Math.Abs(a.Width - b.Width) > Slack || System.Math.Abs(a.Height - b.Height) > Slack) return -1;
        var screen = Pixels(a);
        var right = Pixels(b);
        var (best, bestX, bestY, bestTotal) = (long.MaxValue, 0, 0, 0L);
        for (var dy = -Slack; dy <= Slack; dy++)
        for (var dx = -Slack; dx <= Slack; dx++)
        {
            var (count, opaque) = (0L, 0L);
            for (var y = System.Math.Max(0, -dy); y < a.Height && y + dy < b.Height; y++)
            for (var x = System.Math.Max(0, -dx); x < a.Width && x + dx < b.Width; x++)
            {
                var g = right[(y + dy) * b.Width + x + dx];
                if (!Opaque(g)) continue;
                opaque++;
                if (Off(screen[y * a.Width + x] | unchecked((int)0xFF000000), g, tolerance)) count++;
            }
            if (count < best) (best, bestX, bestY, bestTotal) = (count, dx, dy, opaque);
        }
        total = bestTotal;
        if (best > 0)
        {
            var marked = new int[screen.Length];
            for (var y = 0; y < a.Height; y++)
            for (var x = 0; x < a.Width; x++)
            {
                var (gx, gy) = (x + bestX, y + bestY);
                var i = y * a.Width + x;
                var off = gx >= 0 && gy >= 0 && gx < b.Width && gy < b.Height && Opaque(right[gy * b.Width + gx])
                    && Off(screen[i] | unchecked((int)0xFF000000), right[gy * b.Width + gx], tolerance);
                marked[i] = off ? unchecked((int)0xFFFF0000) : (screen[i] & 0x00FFFFFF) | 0x30000000;
            }
            Write(marked, a.Width, a.Height, diff);
        }
        return best;
    }

    static bool Opaque(int pixel) => ((pixel >> 24) & 0xFF) == 0xFF;

    static void Write(int[] marked, int width, int height, string diff)
    {
        using var picture = new Bitmap(width, height, PixelFormat.Format32bppArgb);
        var data = picture.LockBits(new Rectangle(0, 0, width, height), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
        Marshal.Copy(marked, 0, data.Scan0, marked.Length);
        picture.UnlockBits(data);
        picture.Save(diff, ImageFormat.Png);
    }

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
        if (count > 0) Write(marked, a.Width, a.Height, diff);
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
foreach ($picture in @(Get-ChildItem $Shots -Filter '*-screen.png' | Sort-Object Name)) {
    $name = $picture.Name -replace '-screen\.png$', ''
    $golden = Join-Path $Goldens "$name.png"
    if (-not (Test-Path $golden)) { $failed += "$name on screen has no golden"; continue }
    $diff = Join-Path $Shots "$name-screen-diff.png"
    $total = [long]0
    $count = [GoldenCompare]::DifferingOnScreen($picture.FullName, $golden, $diff, $Tolerance, [ref] $total)
    if ($count -lt 0) { $failed += "$name on screen is more than a few pixels from the golden's size"; continue }
    $share = if ($total -gt 0) { $count / $total } else { 0 }
    "{0} on screen: {1} of {2} opaque pixels differ ({3:P3})" -f $name, $count, $total, $share
    if ($share -gt $Fraction) { $failed += ("{0} on screen differs from its golden in {1:P3} of its opaque pixels; see {2}" -f $name, $share, $diff) }
}
if ($failed.Count -gt 0) { throw "Screenshot goldens failed:`n  $($failed -join "`n  ")" }
"PASS $($pictures.Count) goldens"
