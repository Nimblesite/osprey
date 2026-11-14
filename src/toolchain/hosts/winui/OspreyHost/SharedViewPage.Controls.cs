using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Osprey.Host.WinUI;

/// <summary>
/// The page's controls [NATIVE-SHM-VIEW]: one dispatch from a row's kind to the small method that
/// builds it, and one restyler per kind so a later view updates the same control in place. Every
/// value a builder touches comes from the checked row; nothing here knows an application.
/// </summary>
sealed partial class SharedViewPage
{
    Seat Seated(Shown shown, int index) => shown.Controls[index].Kind switch
    {
        RowKind.Column => Stacked(shown, index, across: false),
        RowKind.Line => Stacked(shown, index, across: true),
        RowKind.Scroll => Scrolled(shown, index),
        RowKind.Layer => Layered(shown, index),
        RowKind.Label => Worded(shown, index, icon: true),
        RowKind.Option => Worded(shown, index, icon: false),
        RowKind.Press => Pressed(shown, index),
        RowKind.Entry => Typed(shown, index),
        RowKind.Editor => Wrote(shown, index),
        RowKind.Gap => new Seat(new Border(), RowKind.Gap),
        RowKind.Toggle => Switched(shown, index),
        RowKind.Picker => Chosen(shown, index),
        RowKind.Rule => Ruled(shown, index),
        RowKind.Image => Pictured(shown, index),
        RowKind.Canvas => Sketched(shown, index),
        RowKind.Menu => Menued(shown, index),
        RowKind.Sheet => Sheeted(shown, index),
        _ => throw new HostException("A row kind the frame reader never lets through"),
    };

    // A column or a line: a card for the row's own look, holding one grid of its children.
    Seat Stacked(Shown shown, int index, bool across)
    {
        var pile = new Grid();
        var card = new Border { Child = pile };
        var seat = new Seat(card, across ? RowKind.Line : RowKind.Column) { Panel = pile, Across = across };
        seat.Restyle = (next, at) => Stacked(card, pile, next.Controls[at], across);
        seat.Restyle(shown, index);
        return seat;
    }

    static void Stacked(Border card, Grid pile, Control row, bool across)
    {
        Faced(card, row);
        Measured(card, row);
        if (across) pile.ColumnSpacing = row.Look.Spacing;
        else pile.RowSpacing = row.Look.Spacing;
    }

    // A column that scrolls when its children outgrow it.
    Seat Scrolled(Shown shown, int index)
    {
        var pile = new Grid();
        var through = new ScrollViewer
        {
            Content = pile,
            VerticalScrollMode = ScrollMode.Enabled,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollMode = ScrollMode.Disabled,
        };
        var card = new Border { Child = through };
        var seat = new Seat(card, RowKind.Scroll) { Panel = pile };
        seat.Restyle = (next, at) => Stacked(card, pile, next.Controls[at], across: false);
        seat.Restyle(shown, index);
        return seat;
    }

    // Children on top of each other: one grid cell holds them all, the last added in front.
    Seat Layered(Shown shown, int index)
    {
        var pile = new Grid();
        var card = new Border { Child = pile };
        var seat = new Seat(card, RowKind.Layer) { Panel = pile };
        seat.Restyle = (next, at) => Stacked(card, pile, next.Controls[at], across: false);
        seat.Restyle(shown, index);
        return seat;
    }

    Seat Worded(Shown shown, int index, bool icon)
    {
        var block = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var words = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        words.Children.Add(block);
        var card = new Border { Child = words };
        var seat = new Seat(card, icon ? RowKind.Label : RowKind.Option);
        seat.Restyle = (next, at) => Worded(block, words, card, next.Controls[at], icon);
        seat.Restyle(shown, index);
        return seat;
    }

    void Worded(TextBlock block, StackPanel words, Border card, Control row, bool icon)
    {
        block.Text = row.Words;
        block.TextAlignment = row.Fit.Centred ? TextAlignment.Center : TextAlignment.DetectFromContent;
        Lettered(block, row);
        Faced(card, row);
        Measured(card, row);
        while (words.Children.Count > 1)
        {
            words.Children.RemoveAt(0);
        }
        if (icon && Iconed(row.Hint) is { } mark)
        {
            words.Children.Insert(0, mark);
        }
    }

    Seat Pressed(Shown shown, int index)
    {
        var button = new Button();
        var seat = new Seat(button, RowKind.Press);
        seat.Restyle = (next, at) => Pressed(button, next.Controls[at]);
        seat.Restyle(shown, index);
        button.Click += (_, _) => control(seat.Action, "");
        return seat;
    }

    void Pressed(Button button, Control row)
    {
        button.Content = row.Words;
        AutomationProperties.SetName(button, row.Words);
        Lettered(button, row);
        Faced(button, row);
        Measured(button, row);
        if (Iconed(row.Hint) is { } mark)
        {
            var words = new TextBlock { Text = row.Words };
            Lettered(words, row);
            button.Content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4, Children = { mark, words } };
        }
    }

    // A field the person types into: a hidden one is another control, so the view rebuilds on it.
    Seat Typed(Shown shown, int index) => shown.Controls[index].Fit.Secure ? Secured(shown, index) : Entered(shown, index);

    Seat Entered(Shown shown, int index)
    {
        var box = new TextBox();
        var seat = new Seat(box, RowKind.Entry);
        seat.Restyle = (next, at) => Entered(box, seat, next.Controls[at]);
        seat.Restyle(shown, index);
        box.TextChanged += (_, _) => { if (!applying && !seat.Commit) control(seat.Action, box.Text); };
        box.KeyDown += (_, args) =>
        {
            if (args.Key == Windows.System.VirtualKey.Enter && !applying) control(seat.Action, box.Text);
        };
        return seat;
    }

    static void Entered(TextBox box, Seat seat, Control row)
    {
        // The same text is not rewritten, so the caret stays where the person left it.
        if (box.Text != row.Words) box.Text = row.Words;
        box.PlaceholderText = row.Hint;
        AutomationProperties.SetName(box, row.Hint.Length == 0 ? row.Words : row.Hint);
        seat.Commit = row.Fit.OnCommit;
        Lettered(box, row);
        Faced(box, row);
        Measured(box, row);
    }

    Seat Secured(Shown shown, int index)
    {
        var box = new PasswordBox();
        var seat = new Seat(box, RowKind.Entry) { Secure = true };
        seat.Restyle = (next, at) => Secured(box, next.Controls[at]);
        seat.Restyle(shown, index);
        box.PasswordChanged += (_, _) => { if (!applying) control(seat.Action, box.Password); };
        return seat;
    }

    static void Secured(PasswordBox box, Control row)
    {
        if (box.Password != row.Words) box.Password = row.Words;
        AutomationProperties.SetName(box, row.Hint.Length == 0 ? row.Words : row.Hint);
        Lettered(box, row);
        Faced(box, row);
        Measured(box, row);
    }

    // A field of many lines, delivering its whole text on every edit.
    Seat Wrote(Shown shown, int index)
    {
        var box = new TextBox { AcceptsReturn = true, TextWrapping = TextWrapping.Wrap };
        ScrollViewer.SetVerticalScrollBarVisibility(box, ScrollBarVisibility.Auto);
        var seat = new Seat(box, RowKind.Editor);
        seat.Restyle = (next, at) => Wrote(box, next.Controls[at]);
        seat.Restyle(shown, index);
        box.TextChanged += (_, _) => { if (!applying) control(seat.Action, box.Text); };
        return seat;
    }

    static void Wrote(TextBox box, Control row)
    {
        if (box.Text != row.Words) box.Text = row.Words;
        AutomationProperties.SetName(box, row.Hint.Length == 0 ? row.Words : row.Hint);
        Lettered(box, row);
        Faced(box, row);
        Measured(box, row);
    }

    Seat Switched(Shown shown, int index)
    {
        var flip = new ToggleSwitch();
        var seat = new Seat(flip, RowKind.Toggle);
        seat.Restyle = (next, at) => Switched(flip, next.Controls[at]);
        seat.Restyle(shown, index);
        flip.Toggled += (_, _) => { if (!applying) control(seat.Action, flip.IsOn ? "true" : "false"); };
        return seat;
    }

    static void Switched(ToggleSwitch flip, Control row)
    {
        flip.Header = row.Words;
        flip.IsOn = row.Fit.On; // restyled under `applying`, so showing the row's state delivers nothing
        AutomationProperties.SetName(flip, row.Words);
        Lettered(flip, row);
        Faced(flip, row);
        Measured(flip, row);
    }

    Seat Chosen(Shown shown, int index)
    {
        var menu = new ComboBox();
        var seat = new Seat(menu, RowKind.Picker); // no panel: its option children become its items
        seat.Restyle = (next, at) => Chosen(menu, next, at);
        seat.Restyle(shown, index);
        menu.SelectionChanged += (_, _) =>
        {
            if (!applying && menu.SelectedItem is ComboBoxItem picked) control(seat.Action, picked.Tag as string ?? "");
        };
        return seat;
    }

    // A menu of the row's option children: each option's text is its label and its hint its key,
    // which choosing delivers. The chosen option is the one its own row flags.
    static void Chosen(ComboBox menu, Shown shown, int index)
    {
        var row = shown.Controls[index];
        menu.Items.Clear();
        var chosen = -1;
        foreach (var child in row.Children)
        {
            var option = shown.Controls[child];
            if (option.Kind is not RowKind.Option)
            {
                continue;
            }
            if (option.Fit.On) chosen = menu.Items.Count;
            menu.Items.Add(new ComboBoxItem { Content = option.Words, Tag = option.Hint, IsEnabled = !option.Fit.Disabled });
        }
        menu.SelectedIndex = chosen;
        AutomationProperties.SetName(menu, row.Words);
        Lettered(menu, row);
        Faced(menu, row);
        Measured(menu, row);
    }

    Seat Ruled(Shown shown, int index)
    {
        var bar = new Border();
        var seat = new Seat(bar, RowKind.Rule);
        seat.Restyle = (next, at) => Ruled(bar, next.Controls[at], Upright(next, at));
        seat.Restyle(shown, index);
        return seat;
    }

    // A hairline in the row's ink, its thickness the row's size; one point when the row says none.
    static void Ruled(Border bar, Control row, bool upright)
    {
        bar.Background = Paint(row.Look.Ink);
        var thick = row.Look.Size > 0 ? row.Look.Size : 1;
        if (upright)
        {
            bar.Width = thick;
            bar.Height = double.NaN;
        }
        else
        {
            bar.Width = double.NaN;
            bar.Height = thick;
        }
    }

    // A rule across a line's children stands upright; anywhere else it lies flat.
    static bool Upright(Shown shown, int index) => shown.Controls[index].Parent is var parent && parent != RowsNone && shown.Controls[parent].Kind == RowKind.Line;

    // The image the row's text names, a file path or an address; a name that is neither draws none.
    Seat Pictured(Shown shown, int index)
    {
        var picture = new Image();
        var card = new Border { Child = picture };
        var seat = new Seat(card, RowKind.Image);
        seat.Restyle = (next, at) => Pictured(picture, card, next.Controls[at]);
        seat.Restyle(shown, index);
        return seat;
    }

    static void Pictured(Image picture, Border card, Control row)
    {
        // Only an absolute address is drawn: a bare relative name is no file path or URL.
        picture.Source = Uri.TryCreate(row.Words, UriKind.Absolute, out var source) ? new BitmapImage(source) : null;
        Faced(card, row);
        Measured(card, row);
    }

    // A canvas draws its scene: the checked rows that name this canvas row as their marker.
    Seat Sketched(Shown shown, int index)
    {
        var scene = new SceneView(problem => report([problem]));
        var seat = new Seat(scene.Element, RowKind.Canvas) { Scene = scene };
        seat.Restyle = (next, at) => Scene(scene, next, at);
        seat.Restyle(shown, index);
        return seat;
    }

    void Scene(SceneView view, Shown shown, int index)
    {
        if (shown.Scenes.TryGetValue(index, out var scene))
        {
            view.Show(scene, shown.Controls[index].Action ?? RowsNone, send);
        }
        else
        {
            view.Clear();
        }
    }

    Seat Menued(Shown shown, int index)
    {
        var pile = new Grid();
        var card = new Border { Child = pile };
        var items = new MenuFlyout();
        card.ContextFlyout = items; // the menu opens on right-click or a long press
        var seat = new Seat(card, RowKind.Menu) { Panel = pile };
        seat.Restyle = (next, at) => Menued(card, items, next, at);
        seat.Restyle(shown, index);
        return seat;
    }

    // The menu's first child is shown; the rest become its context-menu items, each delivering its
    // own action when chosen.
    void Menued(Border card, MenuFlyout items, Shown shown, int index)
    {
        var row = shown.Controls[index];
        Faced(card, row);
        Measured(card, row);
        items.Items.Clear();
        foreach (var child in row.Children.Skip(1))
        {
            var item = shown.Controls[child];
            var entry = new MenuFlyoutItem { Text = item.Words, IsEnabled = !item.Fit.Disabled };
            entry.Click += (_, _) => control(item.Action ?? RowsNone, "");
            items.Items.Add(entry);
        }
    }

    // The sheet's children float above the whole view as a modal sheet; its dismiss press and a tap
    // on its veil both deliver the sheet's own action with no words.
    Seat Sheeted(Shown shown, int index)
    {
        var pile = new Grid();
        var card = new Border
        {
            Child = pile,
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        var veil = new Border { Child = card, Background = SeeThrough };
        var seat = new Seat(veil, RowKind.Sheet) { Panel = pile, Floating = true };
        seat.Restyle = (next, at) => Stacked(card, pile, next.Controls[at], across: false);
        seat.Restyle(shown, index);
        var close = new Button
        {
            HorizontalAlignment = HorizontalAlignment.Right,
            Content = new FontIcon { Glyph = Glyphs.For("dismiss") ?? "" },
        };
        AutomationProperties.SetName(close, "dismiss");
        close.Click += (_, _) => { if (!applying) control(seat.Action, ""); };
        Grid.SetRow(close, 0);
        pile.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        pile.Children.Add(close);
        veil.Tapped += (_, _) => { if (!applying) control(seat.Action, ""); };
        card.Tapped += (_, args) => args.Handled = true; // the sheet itself is not its dismissal
        overlay.Children.Add(veil);
        return seat;
    }

    /// <summary>The icon a label or press is led by, or a visible placeholder for a name this host has no glyph for; null names no icon.</summary>
    FrameworkElement? Iconed(string name)
    {
        if (name.Length == 0)
        {
            return null;
        }
        if (Glyphs.For(name) is { Length: > 0 } glyph)
        {
            return new FontIcon { Glyph = glyph };
        }
        if (!seenIcons.Contains(name))
        {
            seenIcons.Add(name);
            report([$"No icon for symbol {name}"]);
        }
        return new Border { Width = 16, Height = 16, BorderThickness = new Thickness(1), BorderBrush = Paint(0x888888FF) };
    }
}
