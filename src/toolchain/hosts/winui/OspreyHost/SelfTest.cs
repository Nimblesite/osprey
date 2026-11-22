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
/// <c>wait</c> and <c>gone</c> for words, <c>invoke</c> a button or a context-menu command, <c>toggle</c> a switch,
/// <c>choose</c> a row, <c>type</c> into a field, <c>set</c> a slider to a value, <c>drag</c> a scene
/// element by a distance in its units, <c>key</c> to press a key in a scene,
/// <c>frames</c> to let the steady clock run that many 1/60 s frames, <c>sleep</c> for seconds,
/// and <c>shot</c> to save the window as a PNG; on screen, <c>shot</c> also saves the window's client
/// area as the screen shows it, which the goldens check too, and <c>&lt;name&gt;.access.txt</c>, the
/// accessibility tree an agent reads [UI-MACHINE-SELF-TEST]. A failure writes that tree to the log.
/// Any diagnostic the window shows fails the test.
/// </summary>
sealed class SelfTest(HostWindow window, string script, string shots, bool onScreen, TimeSpan patience)
{
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
        var root = window.Content as FrameworkElement;
        if (root is not null) root.LayoutUpdated += Laid;
        CompositionTarget.Rendered += Drew;
        try
        {
            failure = await Perform(Steps(File.ReadAllText(script))) ?? Reported();
        }
        catch (Exception problem) when (problem is IOException or JsonException or InvalidOperationException or KeyNotFoundException)
        {
            failure = problem.Message;
        }
        finally
        {
            if (root is not null) root.LayoutUpdated -= Laid;
            CompositionTarget.Rendered -= Drew;
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
            var named = $"{index + 1:00} {verb} {Logged(verb, words)}";
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

    // A step as the trail records it: what a `type` step enters may be a key, so only its length is kept.
    static string Logged(string verb, string[] words) => verb == "type" && words.Length > 1
        ? $"{words[0]} = ({words[1].Length} characters)"
        : string.Join(" = ", words);

    Task<string?> Do(string verb, string[] words) => verb switch
    {
        "wait" => Until(() => Mentions(words[0]), $"nothing shows \"{words[0]}\""),
        "gone" => Until(() => !Mentions(words[0]), $"\"{words[0]}\" still shows"),
        "invoke" => Act(words[0], Invoke, Invocable, "no enabled control is named"),
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

    // A list remade by a redraw makes its rows a layout pass later, so a picture waits until every shown
    // list has drawn its first row, or the same step would picture an empty list on one run in some.
    async Task<string?> Shot(string name)
    {
        if (await Until(RowsDrawn, "a list never drew its rows") is { } late)
        {
            return late;
        }
        // The first frame of a large scene comes well after its layout, and until it is drawn Windows
        // composes the window blank. A layout that changes nothing visible is never drawn, so the
        // wait for it gives up after a moment instead of failing.
        for (var waited = TimeSpan.Zero; owed && waited < Undrawn; waited += Poll)
        {
            await Task.Delay(Poll);
        }
        await Shoot(name);
        return null;
    }

    static readonly TimeSpan Undrawn = TimeSpan.FromSeconds(3);

    // Whether a layout pass has happened that no drawn frame has shown yet.
    bool owed;

    void Laid(object? sender, object args) => owed = true;

    void Drew(object? sender, RenderedEventArgs args) => owed = false;

    bool RowsDrawn() => window.Content is not { } root || Tree(root).OfType<ListView>().Where(Shown).All(list =>
        list.Items.Count == 0 || list.Items[0] is FrameworkElement { ActualHeight: > 0 } first && VisualTreeHelper.GetParent(first) is not null);

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

    async Task<string?> Act(string words, Func<FrameworkElement, string?> act, Func<string, FrameworkElement?>? finder = null, string absent = "nothing shows")
    {
        var find = finder ?? Find;
        if (await Until(() => find(words) is not null, $"{absent} \"{words}\"") is { } missing)
        {
            return missing;
        }
        try
        {
            return act(find(words)!);
        }
        catch (Exception problem) when (problem is ElementNotEnabledException or InvalidOperationException)
        {
            return problem.Message;
        }
    }

    // A button named in full, or else the first whose name holds the words, as the SwiftUI host reads an
    // `invoke`: a disclosure's title counts what it did ("Ran 1 command, Read 1 file").
    // Only an enabled one counts, as on the SwiftUI host: a step waits for the button to become usable.
    FrameworkElement? Invocable(string words)
    {
        if (window.Content is not { } root) return null;
        if (Find(words) is { } named && Up<ButtonBase>(named) is { IsEnabled: true } exact) return exact;
        var buttons = Tree(root).OfType<ButtonBase>().Where(button => Shown(button) && button.IsEnabled).ToArray();
        return buttons.FirstOrDefault(button => Says(button, words) || Named(button) == words)
            ?? buttons.FirstOrDefault(button => Named(button).Contains(words, StringComparison.Ordinal))
            ?? Commanded(words);
    }

    // An enabled command of that name in a shown element's context menu, in any of its submenus, as a
    // right-click and then a click reach it.
    FrameworkElement? Commanded(string words) => window.Content is { } root
        ? Tree(root).OfType<FrameworkElement>().Where(Shown).Select(element => element.ContextFlyout).OfType<MenuFlyout>()
            .SelectMany(menu => Commands(menu.Items)).FirstOrDefault(entry => entry.IsEnabled && entry.Text == words)
        : null;

    static IEnumerable<MenuFlyoutItem> Commands(IEnumerable<MenuFlyoutItemBase> items) => items.SelectMany(item => item switch
    {
        MenuFlyoutSubItem group => Commands(group.Items),
        MenuFlyoutItem entry => new[] { entry },
        _ => Array.Empty<MenuFlyoutItem>(),
    });

    static string Named(ButtonBase button) => AutomationProperties.GetName(button) is { Length: > 0 } name ? name : button.Content as string ?? "";

    static string? Invoke(FrameworkElement element) => element is MenuFlyoutItem entry
        ? Invoked(() => new MenuFlyoutItemAutomationPeer(entry).Invoke())
        : Up<ButtonBase>(element) switch
    {
        ToggleButton header when Up<Expander>(header) is { } disclosure => Invoked(() => Flipped(new ExpanderAutomationPeer(disclosure))),
        AppBarButton bar => Invoked(() => new AppBarButtonAutomationPeer(bar).Invoke()),
        HyperlinkButton link => Invoked(() => new HyperlinkButtonAutomationPeer(link).Invoke()),
        Button button => Invoked(() => new ButtonAutomationPeer(button).Invoke()),
        _ => "it is not a button",
    };

    // A disclosure opens when shut and shuts when open, as a click on its header does.
    static void Flipped(ExpanderAutomationPeer peer)
    {
        if (peer.ExpandCollapseState == Microsoft.UI.Xaml.Automation.ExpandCollapseState.Expanded)
        {
            peer.Collapse();
        }
        else
        {
            peer.Expand();
        }
    }

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
            TextBox box => $"field \"{(name == "" ? box.Header : name)}\"",
            PasswordBox secure => $"field \"{name}\"",
            ComboBox menu when name != "" => $"picker \"{name}\"",
            AppBarButton bar => $"button \"{bar.Label}\"",
            AppBarToggleButton bar => $"toggle \"{bar.Label}\"",
            ButtonBase button when name != "" => $"button \"{name}\"",
            ButtonBase { Content: string words } => $"button \"{words}\"",
            ListViewItem row => $"row \"{string.Join(" ", Tree(row).OfType<TextBlock>().Select(text => text.Text))}\"",
            _ => null,
        };
    }

    static string? Toggle(FrameworkElement element) => (Up<ToggleSwitch>(element), Up<AppBarToggleButton>(element)) switch
    {
        ({ } toggle, _) => Invoked(() => new ToggleSwitchAutomationPeer(toggle).Toggle()),
        (_, { } bar) => Invoked(() => new AppBarToggleButtonAutomationPeer(bar).Toggle()),
        _ => "it is not a switch",
    };

    static string? Type(FrameworkElement element, string text)
    {
        if (Up<TextBox>(element) is { } box)
        {
            box.Text = text;
            return null;
        }
        if (Up<PasswordBox>(element) is { } secret)
        {
            secret.Password = text;
            return null;
        }
        return "it is not a text field";
    }

    // A row a person would scroll to: when nothing on screen shows `words`, the list holding such a
    // row scrolls it into view, since a list makes rows only as they come into view. A picker's item
    // is chosen through its picker, as a person's click on it would.
    async Task<string?> Chosen(string words)
    {
        if (await Until(() => Find(words) is not null || Listed(words) is not null || Picked(words) is not null, $"no row or picker shows \"{words}\"") is { } missing)
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
        if (Find(words) is null && Listed(words) is { } listed)
        {
            listed.List.ScrollIntoView(listed.Row);
            await Task.Delay(Settle);
        }
        return await Act(words, Choose);
    }

    // A picker whose item shows `words`, whether or not its flyout has made the item yet.
    // As the SwiftUI host reads a `choose`, an exact choice wins over one that only holds the words: its
    // label, a label ending " · <words>" or its key, so `GLM-5.3` picks `GLM · GLM-5.3` before `GLM-5.3 Highspeed`.
    (ComboBox Menu, int Index)? Picked(string words)
    {
        var menus = window.Content is { } root ? Tree(root).OfType<ComboBox>().ToList() : [];
        foreach (var exact in new[] { true, false })
        {
            foreach (var menu in menus)
            {
                var items = menu.Items.OfType<ComboBoxItem>().ToList();
                for (var index = 0; index < items.Count; index++)
                {
                    if (Offers(items[index], words, exact))
                    {
                        return (menu, index);
                    }
                }
            }
        }
        return null;
    }

    static bool Offers(ComboBoxItem item, string words, bool exact)
    {
        var label = item.Content as string ?? "";
        return exact
            ? Holds(item.Content, words) || label.EndsWith(" · " + words, StringComparison.Ordinal) || item.Tag as string == words
            : label.Contains(words, StringComparison.Ordinal);
    }

    // The list and row whose content shows `words`, whether or not the list has made the row yet.
    (ListView List, ListViewItem Row)? Listed(string words)
    {
        var lists = window.Content is { } root ? Tree(root).OfType<ListView>() : Enumerable.Empty<ListView>();
        foreach (var list in lists)
        {
            if (list.Items.OfType<ListViewItem>().FirstOrDefault(row => Holds(row.Content, words)) is { } row)
            {
                return (list, row);
            }
        }
        return null;
    }

    // Whether a row's content holds text showing `words`: its panels, borders and content controls
    // exist as soon as the row is built, before the list makes a visual tree for it.
    static bool Holds(object? content, string words) => content switch
    {
        string text => text == words,
        TextBlock block => block.Text == words,
        Panel panel => panel.Children.Any(child => Holds(child, words)),
        Border border => Holds(border.Child, words),
        ContentControl control => Holds(control.Content, words),
        _ => false,
    };

    // A full collection first, so a row is chosen as it would be long after it was drawn, when the
    // managed wrappers of its list may have been collected and remade.
    string? Choose(FrameworkElement element)
    {
        GC.Collect();
        GC.WaitForPendingFinalizers();
        return Up<ListViewItem>(element) switch
        {
            null => "it is not in a row",
            { } row when window.Choose(row) => null,
            _ => "its row's list takes no choice",
        };
    }

    static string? Invoked(Action action)
    {
        action();
        return null;
    }

    bool Laid(Func<bool> holds)
    {
        window.Content?.UpdateLayout();
        return holds();
    }

    async Task<string?> Until(Func<bool> holds, string failure)
    {
        var clock = Stopwatch.StartNew();
        // A control a redraw just made has no size until layout runs, and would read as not shown.
        while (!Laid(holds))
        {
            if (clock.Elapsed > patience)
            {
                return $"{failure} after {patience.TotalSeconds:0} s";
            }
            await Task.Delay(Poll);
        }
        return null;
    }

    // The first element on screen that shows `words`: a text, a command's label, or an automation name.
    FrameworkElement? Find(string words) =>
        window.Content is { } root ? Tree(root).OfType<FrameworkElement>().FirstOrDefault(element => Shown(element) && Says(element, words)) : null;

    // Whether any shown text or name holds `words`, as the SwiftUI host reads a `wait`; an action still needs its exact name.
    bool Mentions(string words) =>
        window.Content is { } root && Tree(root).OfType<FrameworkElement>().Any(element => Shown(element) && (Spoken(element, words) || Offered(element, words)));

    // A shown picker offers its choices even while it is shut, as the told tree the SwiftUI host reads holds them.
    static bool Offered(FrameworkElement element, string words) =>
        element is ComboBox menu && menu.Items.OfType<ComboBoxItem>().Any(item => (item.Content as string ?? "").Contains(words, StringComparison.Ordinal));

    static bool Spoken(FrameworkElement element, string words) =>
        (element is TextBlock text && text.Text.Contains(words, StringComparison.Ordinal))
        || AutomationProperties.GetName(element).Contains(words, StringComparison.Ordinal)
        || Says(element, words);

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
        var pending = new Stack<DependencyObject>(new DependencyObject[] { root });
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
