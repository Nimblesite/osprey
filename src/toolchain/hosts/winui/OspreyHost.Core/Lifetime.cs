namespace Osprey.Host;

/// <summary>
/// Whether the application still takes events [WINUI-HOST-THREAD]. It ends before the host exits and
/// when its window closes; an event that arrives after that, such as an HTTP answer landing while WinUI
/// tears the window down, is dropped instead of reaching an application whose controls are gone.
/// Used only on the UI thread.
/// </summary>
public sealed class Lifetime
{
    /// <summary>True once the application has ended.</summary>
    public bool Ended { get; private set; }

    /// <summary>Ends the application; ending it again changes nothing.</summary>
    public void End() => Ended = true;

    /// <summary>Runs <paramref name="deliver"/> while the application runs; false when it had ended.</summary>
    public bool Deliver(Action deliver)
    {
        if (Ended)
        {
            return false;
        }
        deliver();
        return true;
    }
}
