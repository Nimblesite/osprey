using System.Diagnostics;
using System.Text;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace Osprey.Host.WinUI;

/// <summary>
/// <c>--self-test</c> [WINUI-HOST-SELF-TEST]: drives the real window from a script that lives beside
/// the application, through the controls' own automation peers, and saves what the window shows.
/// A script names only what a person sees: words on screen, a button's label, a row's text. Steps:
/// <c>wait</c> and <c>gone</c> for words, <c>invoke</c> a button, <c>toggle</c> a switch,
/// <c>choose</c> a row or picker item, <c>type</c> into a field, <c>set</c> a scene's slider,
/// <c>drag</c> a scene element by a distance in its units, <c>key</c> to press a key in a scene,
/// <c>frames</c> to let the steady clock run that many 1/60 s frames, <c>sleep</c> for seconds,
/// and <c>shot</c> to save the window as a PNG; on screen, <c>shot</c> also saves the window's client
/// area as the screen shows it, which the goldens check too, and <c>&lt;name&gt;.access.txt</c>, the
/// accessibility tree an agent reads [UI-MACHINE-SELF-TEST]. A failure writes that tree to the log.
/// Any diagnostic the window shows fails the test.
/// </summary>
sealed class SelfTest(HostWindow window, string script, string shots, bool onScreen)
{
    static readonly TimeSpan Patience = TimeSpan.FromSeconds(90);
    static readonly TimeSpan Poll = TimeSpan.FromMilliseconds(100);
    static readonly TimeSpan Settle = TimeSpan.FromMilliseconds(400);

    readonly StringBuilder log = new();

    /// <summary>Runs every step in order and returns the exit code: 0 when every step passed and nothing was reported.</summary>
    public async Task<int> Run()
    {
        Directory.CreateDirectory(shots);
        // The pictures are drawn at the display's scale, so their goldens are kept per scale [WINUI-HOST-GOLDENS].
        File.WriteAllText(Path.Combine(shots, "scale.txt"), $"{Math.Round(window.Scale * 100)}");
        string? failure;
        try
        {
            failure = await Perform(Steps(File.ReadAllText(script))) ?? Reported();
        }
        catch (Exception problem) when (problem is IOException or JsonException or InvalidOperationException or KeyNotFoundException)
        {
            failure = problem.Message;
        }
        log.AppendLine(failure is null ? "PASS" : $"FAIL {failure}");
        File.WriteAllText(Path.Combine(shots, "self-test.txt"), log.ToString());
        return failure is null ? 0 : 1;
    }

    string? Reported() => window.Problems.Count == 0 ? null : $"the window reported: {string.Join(" | ", window.Problems)}";

    static List<(string Verb, string[] Words)> Steps(string text)
    {
        using var document = JsonDocument.Parse(text);
        return [.. document.RootElement.GetProperty("steps").EnumerateArray().Select(step => Step(step.EnumerateObject().Single()))];
    }

    static (string, string[]) Step(JsonProperty step) => (step.Name, step.Value.ValueKind == JsonValueKind.Array
        ? [.. step.Value.EnumerateArray().Select(item => item.GetString() ?? "")]
        : [step.Value.GetString() ?? ""]);

    async Task<string?> Perform(List<(string Verb, string[] Words)> steps)
    {
        for (var index = 0; index < steps.Count; index++)
        {
            var (verb, words) = steps[index];
            var named = $"{index + 1:00} {verb} {string.Join(" = ", words)}";
            log.AppendLine(named);
            Progress.Note(named);
            if (await Do(verb, words) is { } failure)
            {
                await Shoot($"{index + 1:00}-failed");
                return $"{named}: {failure}{Environment.NewLine}the accessibility tree offers:{Environment.NewLine}{AccessTree()}";
            }
        }
        return null;
    }

    Task<string?> Do(string verb, string[] words) => verb switch
    {
        "wait" => Until(() => Find(words[0]) is not null, $"nothing shows \"{words[0]}\""),
        "gone" => Until(() => Find(words[0]) is null, $"\"{words[0]}\" still shows"),
        "invoke" => Act(words[0], Invoke),
        "toggle" => Act(words[0], Toggle),
        "choose" => Chosen(words[0]),
        "type" => Act(words[0], element => Type(element, words[1])),
        "shot" => Shot(words[0]),
        "frames" => Framed(long.Parse(words[0], System.Globalization.CultureInfo.InvariantCulture)),
        "sleep" => Slept(double.Parse(words[0], System.Globalization.CultureInfo.InvariantCulture)),
        "key" => Task.FromResult(window.Press(words[0]) ? null : "no scene is drawn to take a key"),
        "set" => Act(words[0], element => Set(element, Number(words[1]))),
        "drag" => Act(words[0], _ => window.Drag(words[0], Number(words[1]), Number(words[2])) ? null : "no scene is drawn to take a drag"),
        _ => Task.FromResult<string?>($"there is no step called {verb}"),
    };

    Task<string?> Framed(long frames) => window.Grant(frames)
        ? Until(() => window.Owed == 0, $"{window.Owed} of {frames} frames were not drawn")
        : Task.FromResult<string?>("nothing animates, so no frames run");

    static double Number(string word) => double.Parse(word, System.Globalization.CultureInfo.InvariantCulture);

    static async Task<string?> Slept(double seconds)
    {
        await Task.Delay(TimeSpan.FromSeconds(seconds));
        return null;
    }

    async Task<string?> Shot(string name)
    {
        await Shoot(name);
        return null;
    }

    async Task Shoot(string name)
    {
        await Task.Delay(Settle);
        await Capture.Window(window.Handle, Path.Combine(shots, $"{name}.png"));
        File.WriteAllText(Path.Combine(shots, $"{name}.access.txt"), AccessTree() + Environment.NewLine);
        if (window.Content is { } content)
        {
            await Capture.Content(content, Path.Combine(shots, $"{name}-content.png"));
        }
        if (onScreen)
        {
            await Capture.Screen(window.Handle, Path.Combine(shots, $"{name}-screen.png"));
        }
    }

    async Task<string?> Act(string words, Func<FrameworkElement, string?> act)
    {
        if (await Until(() => Find(words) is not null, $"nothing shows \"{words}\"") is { } missing)
        {
            return missing;
        }
        if (Find(words) is not { } element)
        {
            return $"nothing shows \"{words}\"";
        }
        try
        {
            return act(element);
        }
        catch (Exception problem) when (problem is ElementNotEnabledException or InvalidOperationException)
        {
            return problem.Message;
        }
    }

    static string? Invoke(FrameworkElement element) => Up<ButtonBase>(element) switch
    {
        Button button => Invoked(() => new ButtonAutomationPeer(button).Invoke()),
        _ => "it is not a button",
    };

    static string? Set(FrameworkElement element, double value) => Up<Slider>(element) is { } slider
        ? Invoked(() => new SliderAutomationPeer(slider).SetValue(value))
        : "it is not a slider";

    // What UI Automation announces, in the words the GPUI host's tree uses: one line per thing to act on.
    string AccessTree() => window.Content is { } root
        ? string.Join(Environment.NewLine, Tree(root).OfType<FrameworkElement>().Where(Shown).Select(Announced).OfType<string>())
        : "";

    static string? Announced(FrameworkElement element)
    {
        var name = AutomationProperties.GetName(element);
        return element switch
        {
            Slider slider => $"slider \"{name}\" value {slider.Value} of {slider.Minimum} to {slider.Maximum}",
            ToggleSwitch toggle => $"toggle \"{toggle.Header ?? name}\"",
            TextBox box => $"field \"{(name == "" ? box.PlaceholderText : name)}\"",
            PasswordBox box => $"field \"{name}\"",
            ComboBox menu when name != "" => $"picker \"{name}\"",
            ButtonBase button when name != "" => $"button \"{name}\"",
            ButtonBase { Content: string words } => $"button \"{words}\"",
            _ => null,
        };
    }

    static string? Toggle(FrameworkElement element) => Up<ToggleSwitch>(element) switch
    {
        { } toggle => Invoked(() => new ToggleSwitchAutomationPeer(toggle).Toggle()),
        _ => "it is not a switch",
    };

    static string? Type(FrameworkElement element, string text)
    {
        if (Up<TextBox>(element) is not { } box)
        {
            return "it is not a text field";
        }
        box.Text = text;
        return null;
    }

    // A choice is a picker's item when the words name one; otherwise it is a row's own press, which
    // the words are drawn inside, so clicking it delivers the row's key exactly as a hand would.
    async Task<string?> Chosen(string words)
    {
        if (await Until(() => Find(words) is not null || Picked(words) is not null, $"no row or picker shows \"{words}\"") is { } missing)
        {
            return missing;
        }
        if (Picked(words) is { } picker)
        {
            GC.Collect();
            GC.WaitForPendingFinalizers();
            picker.Menu.SelectedIndex = picker.Index;
            return null;
        }
        return Find(words) is { } element && PressOver(element) is { } press
            ? Invoked(() => new ButtonAutomationPeer(press).Invoke())
            : $"nothing presses \"{words}\"";
    }

    // The press that covers the element: a button it sits in, or the sibling button a row draws over
    // its body, so choosing a row delivers the row's key as a hand's click does.
    static Button? PressOver(FrameworkElement element)
    {
        for (DependencyObject? node = element; node is not null; node = VisualTreeHelper.GetParent(node))
        {
            if (node is Button button)
            {
                return button;
            }
            if (node is Panel panel && panel.Children.OfType<Button>().FirstOrDefault(child => child.Visibility == Visibility.Visible) is { } over)
            {
                return over;
            }
        }
        return null;
    }

    // A picker whose item shows `words`, whether or not its flyout has made the item yet.
    (ComboBox Menu, int Index)? Picked(string words)
    {
        var menus = window.Content is { } root ? Tree(root).OfType<ComboBox>() : Enumerable.Empty<ComboBox>();
        foreach (var menu in menus)
        {
            var items = menu.Items.OfType<ComboBoxItem>().ToList();
            for (var index = 0; index < items.Count; index++)
            {
                if (Holds(items[index].Content, words))
                {
                    return (menu, index);
                }
            }
        }
        return null;
    }

    // Whether a picker item's content holds text showing `words`: its panels and borders exist as
    // soon as the item is built, before the flyout makes a visual tree for it.
    static bool Holds(object? content, string words) => content switch
    {
        string text => text == words,
        TextBlock text => text.Text == words,
        Panel panel => panel.Children.Any(child => Holds(child, words)),
        Border border => Holds(border.Child, words),
        ContentControl control => Holds(control.Content, words),
        _ => false,
    };

    static string? Invoked(Action action)
    {
        action();
        return null;
    }

    static async Task<string?> Until(Func<bool> holds, string failure)
    {
        var clock = Stopwatch.StartNew();
        while (!holds())
        {
            if (clock.Elapsed > Patience)
            {
                return $"{failure} after {Patience.TotalSeconds:0} s";
            }
            await Task.Delay(Poll);
        }
        return null;
    }

    // The first element on screen that shows `words`: a text, a command's label, or an automation name.
    FrameworkElement? Find(string words) =>
        window.Content is { } root ? Tree(root).OfType<FrameworkElement>().FirstOrDefault(element => Shown(element) && Says(element, words)) : null;

    static bool Says(FrameworkElement element, string words) => element switch
    {
        TextBlock text => text.Text == words,
        ButtonBase { Content: string label } => label == words,
        _ => AutomationProperties.GetName(element) == words,
    };

    static bool Shown(FrameworkElement element) => element.Visibility == Visibility.Visible && element.ActualWidth > 0 && element.ActualHeight > 0;

    static IEnumerable<DependencyObject> Tree(DependencyObject root)
    {
        var pending = new Stack<DependencyObject>([root]);
        while (pending.TryPop(out var node))
        {
            yield return node;
            for (var index = VisualTreeHelper.GetChildrenCount(node) - 1; index >= 0; index--)
            {
                pending.Push(VisualTreeHelper.GetChild(node, index));
            }
        }
    }

    static T? Up<T>(DependencyObject? element) where T : class
    {
        for (; element is not null; element = VisualTreeHelper.GetParent(element))
        {
            if (element is T found)
            {
                return found;
            }
        }
        return null;
    }
}
