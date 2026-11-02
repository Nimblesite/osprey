# Screenshot goldens [WINUI-HOST-GOLDENS]: compares each picture of the window's content a self-test
# took (<name>-content.png, drawn without the desktop behind the window) with the golden of the same
# name, allowing any pixel to land one pixel over, as WinUI's rounding of layout can move text and
# edges by one pixel between runs. WinUI draws at the display's scale, so the goldens for a scale are in <Goldens>/<percent>,
# the percent the self-test wrote to scale.txt: <app>/goldens/windows/150 at 150%. A pixel differs when any channel is off by more than -Tolerance; a picture fails when more
# than -Fraction of its pixels differ or its size differs, and a diff picture is written beside it.
# A shot with no golden fails too. -Update replaces the goldens with these pictures.
# A self-test on screen also keeps <name>-screen.png, the window's client area as the screen shows it;
# it is held to the same golden drawn over the backdrop, every pixel: where the content is clear the
# screen shows the Mica backdrop, which follows the wallpaper, so the backdrop is taken from the screen
# itself. The two are lined up within a few pixels, since the content's picture can reach a little past
# the area it is laid out in. So what a person sees is checked too.
# A live.json beside a scale's goldens lists [x, y, width, height] rectangles of the golden that show the
# machine rather than the app, such as how full its disk is. Those pixels are not compared; -Update keeps the file.
#   goldens.ps1 -Shots <folder> -Goldens <folder> [-Update] [-Tolerance 24] [-Fraction 0.002]
param(
    [Parameter(Mandatory)] [string] $Shots,
    [Parameter(Mandatory)] [string] $Goldens,
    [switch] $Update,
    [int] $Tolerance = 24,
    [double] $Fraction = 0.002
)
$ErrorActionPreference = 'Stop'
$scale = Join-Path $Shots 'scale.txt'
if (-not (Test-Path $scale)) { throw "$Shots has no scale.txt; the self-test writes it with the display's scale" }
$Goldens = Join-Path $Goldens (Get-Content $scale -Raw).Trim()
$pictures = @(Get-ChildItem $Shots -Filter '*-content.png' | Sort-Object Name)
if ($pictures.Count -eq 0) { throw "no pictures of the window's content in $Shots" }
if ($Update) {
    New-Item -ItemType Directory -Force $Goldens | Out-Null
    Get-ChildItem $Goldens -Filter *.png | Remove-Item
    $pictures | ForEach-Object { Copy-Item $_.FullName (Join-Path $Goldens ($_.Name -replace '-content\.png$', '.png')) }
    "updated $($pictures.Count) goldens in $Goldens"
    return
}

Add-Type -ReferencedAssemblies System.Collections, System.Drawing.Common, System.Drawing.Primitives, System.Private.Windows.GdiPlus, System.Private.Windows.Core -TypeDefinition @'
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class GoldenCompare
{
    // The rectangles of the golden that are not compared, four numbers each: x, y, width, height.
    public static int[] Live = new int[0];

    static bool IsLive(int x, int y)
    {
        for (var r = 0; r < Live.Length; r += 4)
        {
            if (x >= Live[r] && y >= Live[r + 1] && x < Live[r] + Live[r + 2] && y < Live[r + 1] + Live[r + 3]) return true;
        }
        return false;
    }

    // The count of pixels that differ by more than tolerance in any channel, with each one painted
    // red in the diff and the rest faded; -1 when the sizes differ.
    // The pixels of a picture of the screen that differ from the golden drawn over the backdrop, at the
    // offset of at most Slack pixels each way that lines up the golden's opaque pixels best; the
    // screen's content area can sit a few pixels inside the content's own picture. The backdrop is
    // the screen's mean colour where the golden is clear, so every pixel is checked: text and
    // translucent controls as they blend with it, and clear areas where nothing else may show. The
    // pixels compared come back through total, and -1 when the sizes are further apart than the slack.
    public static long DifferingOnScreen(string shot, string golden, string diff, int tolerance, out long total)
    {
        const int Slack = 8;
        using var a = new Bitmap(shot);
        using var b = new Bitmap(golden);
        total = 0;
        if (System.Math.Abs(a.Width - b.Width) > Slack || System.Math.Abs(a.Height - b.Height) > Slack) return -1;
        var screen = Pixels(a);
        var right = Pixels(b);
        // Only the golden's opaque pixels decide the offset, and every one counts, as a mostly clear
        // golden has few. The nearest offsets go first, so a tie keeps the smaller shift and a clearly
        // wrong offset stops as soon as it cannot win.
        var opaque = new System.Collections.Generic.List<int>();
        for (var i = 0; i < right.Length; i++)
        {
            if (Opaque(right[i]) && !IsLive(i % b.Width, i / b.Width)) opaque.Add(i);
        }
        var offsets = new System.Collections.Generic.List<(int X, int Y)>();
        for (var dy = -Slack; dy <= Slack; dy++)
        for (var dx = -Slack; dx <= Slack; dx++) offsets.Add((dx, dy));
        offsets.Sort((p, q) => (System.Math.Abs(p.X) + System.Math.Abs(p.Y)).CompareTo(System.Math.Abs(q.X) + System.Math.Abs(q.Y)));
        var (best, bestX, bestY) = (long.MaxValue, 0, 0);
        foreach (var (dx, dy) in offsets)
        {
            var count = 0L;
            foreach (var i in opaque)
            {
                var (x, y) = (i % b.Width - dx, i / b.Width - dy);
                if (x < 0 || y < 0 || x >= a.Width || y >= a.Height) continue;
                if (Off(screen[y * a.Width + x] | unchecked((int)0xFF000000), right[i], tolerance) && ++count >= best) break;
            }
            if (count < best) (best, bestX, bestY) = (count, dx, dy);
        }
        var backdrop = Backdrop(screen, a.Width, a.Height, right, b.Width, b.Height, bestX, bestY);
        var marked = new int[screen.Length];
        var differing = 0L;
        for (var y = 0; y < a.Height; y++)
        for (var x = 0; x < a.Width; x++)
        {
            var (gx, gy) = (x + bestX, y + bestY);
            var i = y * a.Width + x;
            var inside = gx >= 0 && gy >= 0 && gx < b.Width && gy < b.Height;
            var off = inside && !IsLive(gx, gy) && !Near(screen[i] | unchecked((int)0xFF000000), right, b.Width, b.Height, gx, gy, tolerance, backdrop);
            total += inside ? 1 : 0;
            differing += off ? 1 : 0;
            marked[i] = off ? unchecked((int)0xFFFF0000) : (screen[i] & 0x00FFFFFF) | 0x30000000;
        }
        if (differing > 0) Write(marked, a.Width, a.Height, diff);
        return differing;
    }

    // The screen's mean colour where the golden is clear: the backdrop the content is drawn over.
    static int Backdrop(int[] screen, int width, int height, int[] golden, int goldenWidth, int goldenHeight, int dx, int dy)
    {
        long red = 0, green = 0, blue = 0, count = 0;
        for (var y = System.Math.Max(0, -dy); y < height && y + dy < goldenHeight; y++)
        for (var x = System.Math.Max(0, -dx); x < width && x + dx < goldenWidth; x++)
        {
            if (((golden[(y + dy) * goldenWidth + x + dx] >> 24) & 0xFF) != 0) continue;
            var p = screen[y * width + x];
            (red, green, blue, count) = (red + ((p >> 16) & 0xFF), green + ((p >> 8) & 0xFF), blue + (p & 0xFF), count + 1);
        }
        return count == 0 ? unchecked((int)0xFFFFFFFF) : unchecked((int)0xFF000000) | (int)(red / count) << 16 | (int)(green / count) << 8 | (int)(blue / count);
    }

    // Whether the golden has a pixel matching this one at x, y or one pixel away: WinUI rounds layout
    // to whole pixels a little differently from run to run and between its picture and the screen,
    // so text and edges can land a pixel over; any real change spans more than that. With a backdrop,
    // the golden's pixels are first drawn over it, as the screen composes them.
    static bool Near(int pixel, int[] golden, int width, int height, int x, int y, int tolerance, int? backdrop)
    {
        for (var ny = System.Math.Max(0, y - 1); ny <= System.Math.Min(height - 1, y + 1); ny++)
        for (var nx = System.Math.Max(0, x - 1); nx <= System.Math.Min(width - 1, x + 1); nx++)
        {
            var g = golden[ny * width + nx];
            if (!Off(pixel, backdrop is { } under ? Over(g, under) : g, tolerance)) return true;
        }
        return false;
    }

    // The golden's pixel blended over the backdrop, as the screen composes it.
    static int Over(int pixel, int backdrop)
    {
        var alpha = ((pixel >> 24) & 0xFF) / 255.0;
        int Channel(int shift) => (int)System.Math.Round(((pixel >> shift) & 0xFF) * alpha + ((backdrop >> shift) & 0xFF) * (1 - alpha));
        return unchecked((int)0xFF000000) | Channel(16) << 16 | Channel(8) << 8 | Channel(0);
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
            var off = !IsLive(i % a.Width, i / a.Width) && !Near(left[i], right, a.Width, a.Height, i % a.Width, i / a.Width, tolerance, null);
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

$live = Join-Path $Goldens 'live.json'
if (Test-Path $live) { [GoldenCompare]::Live = [int[]] @(Get-Content $live -Raw | ConvertFrom-Json | ForEach-Object { $_ }) }
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
    "{0} on screen: {1} of {2} pixels differ ({3:P3})" -f $name, $count, $total, $share
    if ($share -gt $Fraction) { $failed += ("{0} on screen differs from its golden in {1:P3} of its pixels; see {2}" -f $name, $share, $diff) }
}
if ($failed.Count -gt 0) { throw "Screenshot goldens failed:`n  $($failed -join "`n  ")" }
"PASS $($pictures.Count) goldens"
