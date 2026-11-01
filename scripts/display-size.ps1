# Sets the primary display's size, for a runner whose desktop is too small for the windows it tests.
#   display-size.ps1 -Width <pixels> -Height <pixels>
param(
    [Parameter(Mandatory)] [int] $Width,
    [Parameter(Mandatory)] [int] $Height
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class Display
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct Mode
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string Device;
        public short SpecVersion, DriverVersion, Size, DriverExtra;
        public int Fields, PositionX, PositionY, Orientation, FixedOutput;
        public short Color, Duplex, YResolution, TrueType, Collate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string Form;
        public short LogPixels;
        public int BitsPerPixel, Width, Height, Flags, Frequency, IcmMethod, IcmIntent, MediaType, DitherType, Reserved1, Reserved2, PanningWidth, PanningHeight;
    }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool EnumDisplaySettings(string device, int mode, ref Mode found);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int ChangeDisplaySettings(ref Mode wanted, int flags);
    public static int Set(int width, int height)
    {
        var mode = new Mode();
        mode.Size = (short)Marshal.SizeOf(typeof(Mode));
        if (!EnumDisplaySettings(null, -1, ref mode)) return -100;
        mode.Width = width;
        mode.Height = height;
        mode.Fields = 0x80000 | 0x100000;
        return ChangeDisplaySettings(ref mode, 0);
    }
}
'@
$status = [Display]::Set($Width, $Height)
$screen = [Windows.Forms.Screen]::PrimaryScreen.Bounds
"The display is $($screen.Width) by $($screen.Height) (status $status)"
if ($status -ne 0) { throw "The display would not take $Width by $Height (status $status)" }
