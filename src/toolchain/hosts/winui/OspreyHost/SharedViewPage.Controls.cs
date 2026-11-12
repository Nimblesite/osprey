using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Osprey.Host.WinUI;

/// <summary>
/// The page's controls [NATIVE-SHM-VIEW]: one dispatch from a row's kind to the small method that
/// builds it, and one restyler per kind so a later view updates the same control in place. Every
/// value a builder touches comes from the row; nothing here knows an application.
/// </summary>
sealed partial class SharedViewPage
{
    Seat? Seated(Rows rows, int index) => rows.At(index).Kind switch
    {
        RowKind.Column => Stacked(rows, index, across: false),
        RowKind.Line => Stacked(rows, index, across: true),
        RowKind.Scroll => Scrolled(rows, index),
        RowKind.Layer => Layered(rows, index),
        RowKind.Label => Worded(rows, index),
        RowKind.Option => Optioned(rows, index),
        RowKind.Press => Pressed(rows, index),
        RowKind.Entry => Typed(rows, index),
        RowKind.Editor => Wrote(rows, index),
        RowKind.Gap => new Seat(new Border(), RowKind.Gap),
        RowKind.Toggle => Switched(rows, index),
        RowKind.Picker => Chosen(rows, index),
        RowKind.Rule => Ruled(rows, index),
        RowKind.Image => Pictured(rows, index),
        RowKind.Canvas => Reserved(rows, index),
        RowKind.Menu => Menued(rows, index),
        RowKind.Sheet => Sheeted(rows, index),
        _ => null, // Rows never seats a row of unknown kind; this keeps the dispatch total
    };

    // A column or a line: a card for the row's own look, holding one grid of its children.
    Seat Stacked(Rows rows, int index, bool across)
    {
        var pile = new Grid();
        var card = new Border { Child = pile };
        var seat = new Seat(card, across ? RowKind.Line : RowKind.Column) { Panel = pile, Across = across };
        seat.Restyle = (next, at) => Stacked(card, pile, next.At(at), across);
        seat.Restyle(rows, index);
        return seat;
    }

    static void Stacked(Border card, Grid pile, SafeRow row, bool across)
    {
        Faced(card, row);
        Measured(card, row);
        if (across) pile.ColumnSpacing = row.Spacing;
        else pile.RowSpacing = row.Spacing;
    }

    // A column that scrolls when its children outgrow it.
    Seat Scrolled(Rows rows, int index)
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
        seat.Restyle = (next, at) => Stacked(card, pile, next.At(at), across: false);
        seat.Restyle(rows, index);
        return seat;
    }

    // Children on top of each other: one grid cell holds them all, the last added in front.
    Seat Layered(Rows rows, int index)
    {
        var pile = new Grid();
        var card = new Border { Child = pile };
        var seat = new Seat(card, RowKind.Layer) { Panel = pile };
        seat.Restyle = (next, at) => Stacked(card, pile, next.At(at), across: false);
        seat.Restyle(rows, index);
        return seat;
    }

    Seat Worded(Rows rows, int index) => Worded(rows, index, RowKind.Label);

    // An option outside its picker reads as the label it carries.
    Seat Optioned(Rows rows, int index) => Worded(rows, index, RowKind.Option);

    Seat Worded(Rows rows, int index, RowKind kind)
    {
        var block = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var card = new Border { Child = block };
        var seat = new Seat(card, kind);
        seat.Restyle = (next, at) => Worded(block, card, next.At(at));
        seat.Restyle(rows, index);
        return seat;
    }

    static void Worded(TextBlock block, Border card, SafeRow row)
    {
        block.Text = row.Text;
        block.TextAlignment = row.Centred ? TextAlignment.Center : TextAlignment.DetectFromContent;
        Lettered(block, row);
        Faced(card, row);
        Measured(card, row);
    }

    Seat Pressed(Rows rows, int index)
    {
        var button = new Button();
        var seat = new Seat(button, RowKind.Press);
        seat.Restyle = (next, at) => Pressed(button, next.At(at));
        seat.Restyle(rows, index);
        button.Click += (_, _) => Send(seat.Action, "");
        return seat;
    }

    static void Pressed(Button button, SafeRow row)
    {
        button.Content = row.Text;
        Lettered(button, row);
        Faced(button, row);
        Measured(button, row);
    }

    // A field the person types into: a hidden one is another control, so the view rebuilds on it.
    Seat Typed(Rows rows, int index) => rows.At(index).Secure ? Secured(rows, index) : Entered(rows, index);

    Seat Entered(Rows rows, int index)
    {
        var box = new TextBox();
        var seat = new Seat(box, RowKind.Entry);
        seat.Restyle = (next, at) => Entered(box, seat, next.At(at));
        seat.Restyle(rows, index);
        box.TextChanged += (_, _) => { if (!applying && !seat.Commit) Send(seat.Action, box.Text); };
        box.KeyDown += (_, args) =>
        {
            if (args.Key == Windows.System.VirtualKey.Enter && !applying) Send(seat.Action, box.Text);
        };
        return seat;
    }

    static void Entered(TextBox box, Seat seat, SafeRow row)
    {
        // The same text is not rewritten, so the caret stays where the person left it.
        if (box.Text != row.Text) box.Text = row.Text;
        box.PlaceholderText = row.Hint;
        AutomationProperties.SetName(box, string.IsNullOrEmpty(row.Hint) ? row.Text : row.Hint);
        seat.Commit = row.OnCommit;
        Lettered(box, row);
        Faced(box, row);
        Measured(box, row);
    }

    Seat Secured(Rows rows, int index)
    {
        var box = new PasswordBox();
        var seat = new Seat(box, RowKind.Entry) { Secure = true };
        seat.Restyle = (next, at) => Secured(box, next.At(at));
        seat.Restyle(rows, index);
        box.PasswordChanged += (_, _) => { if (!applying) Send(seat.Action, box.Password); };
        return seat;
    }

    static void Secured(PasswordBox box, SafeRow row)
    {
        if (box.Password != row.Text) box.Password = row.Text;
        AutomationProperties.SetName(box, row.Text);
        Lettered(box, row);
        Faced(box, row);
        Measured(box, row);
    }

    // A field of many lines, delivering its whole text on every edit.
    Seat Wrote(Rows rows, int index)
    {
        var box = new TextBox { AcceptsReturn = true, TextWrapping = TextWrapping.Wrap };
        ScrollViewer.SetVerticalScrollBarVisibility(box, ScrollBarVisibility.Auto);
        var seat = new Seat(box, RowKind.Editor);
        seat.Restyle = (next, at) => Wrote(box, next.At(at));
        seat.Restyle(rows, index);
        box.TextChanged += (_, _) => { if (!applying) Send(seat.Action, box.Text); };
        return seat;
    }

    static void Wrote(TextBox box, SafeRow row)
    {
        if (box.Text != row.Text) box.Text = row.Text;
        Lettered(box, row);
        Faced(box, row);
        Measured(box, row);
    }

    Seat Switched(Rows rows, int index)
    {
        var flip = new ToggleSwitch();
        var seat = new Seat(flip, RowKind.Toggle);
        seat.Restyle = (next, at) => Switched(flip, next.At(at));
        seat.Restyle(rows, index);
        flip.Toggled += (_, _) => { if (!applying) Send(seat.Action, flip.IsOn ? "true" : "false"); };
        return seat;
    }

    static void Switched(ToggleSwitch flip, SafeRow row)
    {
        flip.Header = row.Text;
        flip.IsOn = row.On; // restyled under `applying`, so showing the row's state delivers nothing
        Lettered(flip, row);
        Faced(flip, row);
        Measured(flip, row);
    }

    Seat Chosen(Rows rows, int index)
    {
        var menu = new ComboBox();
        var seat = new Seat(menu, RowKind.Picker); // no panel: its option children become its items
        seat.Restyle = (next, at) => Chosen(menu, next, at);
        seat.Restyle(rows, index);
        menu.SelectionChanged += (_, _) =>
        {
            if (!applying && menu.SelectedItem is ComboBoxItem picked) Send(seat.Action, picked.Tag as string ?? "");
        };
        return seat;
    }

    // A menu of the row's option children: each option's text is its label and its hint its key,
    // which choosing delivers. The chosen option is the one its own row flags.
    static void Chosen(ComboBox menu, Rows rows, int index)
    {
        var row = rows.At(index);
        menu.Items.Clear();
        var chosen = -1;
        foreach (var child in rows.Children(index))
        {
            var option = rows.At(child);
            if (option.Kind is not RowKind.Option)
            {
                continue;
            }
            if (option.On) chosen = menu.Items.Count;
            menu.Items.Add(new ComboBoxItem { Content = option.Text, Tag = option.Hint, IsEnabled = !option.Disabled });
        }
        menu.SelectedIndex = chosen;
        Lettered(menu, row);
        Faced(menu, row);
        Measured(menu, row);
    }

    Seat Ruled(Rows rows, int index)
    {
        var bar = new Border();
        var seat = new Seat(bar, RowKind.Rule);
        seat.Restyle = (next, at) => Ruled(bar, next.At(at), Upright(next, at));
        seat.Restyle(rows, index);
        return seat;
    }

    // A hairline in the row's ink, its thickness the row's size; one point when the row says none.
    static void Ruled(Border bar, SafeRow row, bool upright)
    {
        bar.Background = Paint(row.Colour);
        var thick = row.Size > 0 ? row.Size : 1;
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
    static bool Upright(Rows rows, int index)
    {
        var parent = rows.At(index).Parent;
        return parent != Rows.None && rows.At(parent).Kind == RowKind.Line;
    }

    // The image the row's text names, a file path or an address; a name that is neither draws none.
    Seat Pictured(Rows rows, int index)
    {
        var picture = new Image();
        var card = new Border { Child = picture };
        var seat = new Seat(card, RowKind.Image);
        seat.Restyle = (next, at) => Pictured(picture, card, next.At(at));
        seat.Restyle(rows, index);
        return seat;
    }

    static void Pictured(Image picture, Border card, SafeRow row)
    {
        // Only an absolute address is drawn: a bare relative name is no file path or URL.
        picture.Source = Uri.TryCreate(row.Text, UriKind.Absolute, out var source) ? new BitmapImage(source) : null;
        Faced(card, row);
        Measured(card, row);
    }

    // A canvas draws nothing yet: the row's fixed extents are kept clear for the shapes to come.
    Seat Reserved(Rows rows, int index)
    {
        var blank = new Canvas();
        var card = new Border { Child = blank };
        var seat = new Seat(card, RowKind.Canvas);
        seat.Restyle = (next, at) => Reserved(card, next.At(at));
        seat.Restyle(rows, index);
        return seat;
    }

    static void Reserved(Border card, SafeRow row)
    {
        Measured(card, row);
        Faced(card, row);
    }

    Seat Menued(Rows rows, int index)
    {
        var pile = new Grid();
        var card = new Border { Child = pile };
        var items = new MenuFlyout();
        card.ContextFlyout = items; // the menu opens on right-click or a long press
        var seat = new Seat(card, RowKind.Menu) { Panel = pile };
        seat.Restyle = (next, at) => Menued(card, items, next, at);
        seat.Restyle(rows, index);
        return seat;
    }

    // The menu's first child is shown; the rest become its context-menu items, each delivering its
    // own action when chosen.
    void Menued(Border card, MenuFlyout items, Rows rows, int index)
    {
        var row = rows.At(index);
        Faced(card, row);
        Measured(card, row);
        items.Items.Clear();
        foreach (var child in rows.Children(index).Skip(1))
        {
            var item = rows.At(child);
            var entry = new MenuFlyoutItem { Text = item.Text, IsEnabled = !item.Disabled };
            entry.Click += (_, _) => Send(item.Action, "");
            items.Items.Add(entry);
        }
    }

    // The sheet's children float above the whole view as a modal sheet; tapping it away delivers
    // the sheet's own action with no words.
    Seat Sheeted(Rows rows, int index)
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
        seat.Restyle = (next, at) => Stacked(card, pile, next.At(at), across: false);
        seat.Restyle(rows, index);
        veil.Tapped += (_, _) => { if (!applying) Send(seat.Action, ""); };
        card.Tapped += (_, args) => args.Handled = true; // the sheet itself is not its dismissal
        overlay.Children.Add(veil);
        return seat;
    }
}
