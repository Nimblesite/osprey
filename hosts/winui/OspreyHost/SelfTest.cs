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
/// <c>choose</c> a row, <c>type</c> into a field, and <c>shot</c> to save the window as a PNG.
/// Any diagnostic the window shows fails the test.
/// </summary>
sealed class SelfTest(HostWindow window, string script, string shots)
{
    static readonly TimeSpan Patience = TimeSpan.FromSeconds(90);
    static readonly TimeSpan Poll = TimeSpan.FromMilliseconds(100);
    static readonly TimeSpan Settle = TimeSpan.FromMilliseconds(400);

    readonly StringBuilder log = new();

    /// <summary>Runs every step in order and returns the exit code: 0 when every step passed and nothing was reported.</summary>
    public async Task<int> Run()
    {
        Directory.CreateDirectory(shots);
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
                return $"{named}: {failure}";
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
        "choose" => Act(words[0], element => Up<ListViewItem>(element) is { } row && window.Choose(row) ? null : "it is not in a row"),
        "type" => Act(words[0], element => Type(element, words[1])),
        "shot" => Shot(words[0]),
        _ => Task.FromResult<string?>($"there is no step called {verb}"),
    };

    async Task<string?> Shot(string name)
    {
        await Shoot(name);
        return null;
    }

    async Task Shoot(string name)
    {
        await Task.Delay(Settle);
        await Capture.Window(window.Handle, Path.Combine(shots, $"{name}.png"));
        if (window.Content is { } content)
        {
            await Capture.Content(content, Path.Combine(shots, $"{name}-content.png"));
        }
    }

    async Task<string?> Act(string words, Func<FrameworkElement, string?> act)
    {
        if (await Until(() => Find(words) is not null, $"nothing shows \"{words}\"") is { } missing)
        {
            return missing;
        }
        try
        {
            return act(Find(words)!);
        }
        catch (Exception problem) when (problem is ElementNotEnabledException or InvalidOperationException)
        {
            return problem.Message;
        }
    }

    static string? Invoke(FrameworkElement element) => Up<ButtonBase>(element) switch
    {
        AppBarButton bar => Invoked(() => new AppBarButtonAutomationPeer(bar).Invoke()),
        HyperlinkButton link => Invoked(() => new HyperlinkButtonAutomationPeer(link).Invoke()),
        Button button => Invoked(() => new ButtonAutomationPeer(button).Invoke()),
        _ => "it is not a button",
    };

    static string? Toggle(FrameworkElement element) => (Up<ToggleSwitch>(element), Up<AppBarToggleButton>(element)) switch
    {
        ({ } toggle, _) => Invoked(() => new ToggleSwitchAutomationPeer(toggle).Toggle()),
        (_, { } bar) => Invoked(() => new AppBarToggleButtonAutomationPeer(bar).Toggle()),
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
        AppBarButton bar => bar.Label == words,
        AppBarToggleButton bar => bar.Label == words,
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
