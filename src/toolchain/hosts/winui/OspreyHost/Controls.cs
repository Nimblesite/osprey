using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.System;

namespace Osprey.Host.WinUI;

/// <summary>
/// Controls [UI-EVENTS]: each sends one value under the ordinal the told record carries. A control's
/// handler is attached after its value is set, so drawing never sends an event.
/// </summary>
sealed partial class Renderer
{
    UIElement Button(Scope scope, Node node)
    {
        var button = new Button { Content = Line(Glyph(node.Text("symbol")), Words(node.Text("label"))), IsEnabled = node.Flag("enabled") };
        if (node.Text("role") == "prominent")
        {
            button.Style = Style("AccentButtonStyle");
        }
        if (node.Text("role") == "destructive")
        {
            button.Foreground = Tone("danger");
        }
        var emit = Emit(node);
        button.Click += (_, _) => emit(null);
        return button;
    }

    /// <summary>A text box committing every edit, or only on Enter; a secure field is a password box.</summary>
    UIElement Field(Scope scope, Node node)
    {
        var emit = Emit(node);
        if (node.Flag("secure"))
        {
            var secret = new PasswordBox { Header = node.Text("label"), PlaceholderText = node.Text("prompt"), Password = node.Text("value") };
            secret.PasswordChanged += (_, _) => emit(secret.Password);
            return secret;
        }
        var box = new TextBox { Header = node.Text("label"), PlaceholderText = node.Text("prompt"), Text = node.Text("value"), AcceptsReturn = node.Flag("multiline") };
        if (node.Text("commit") == "submit")
        {
            box.KeyDown += (_, args) => { if (args.Key == VirtualKey.Enter) { emit(box.Text); } };
        }
        else
        {
            box.TextChanged += (_, _) => emit(box.Text);
        }
        return box;
    }

    UIElement Toggle(Scope scope, Node node)
    {
        var toggle = new ToggleSwitch { Header = node.Text("label"), IsOn = node.Flag("on") };
        var emit = Emit(node);
        toggle.Toggled += (_, _) => emit(toggle.IsOn);
        return toggle;
    }

    UIElement Picker(Scope scope, Node node)
    {
        var box = new ComboBox { Header = node.Text("label") };
        foreach (var choice in node.Items("choices"))
        {
            box.Items.Add(new ComboBoxItem { Content = choice.Text("label"), Tag = choice.Text("key") });
        }
        box.SelectedItem = Tagged<ComboBoxItem>(box.Items, node.Text("selected"));
        var emit = Emit(node);
        box.SelectionChanged += (_, _) => emit((box.SelectedItem as ComboBoxItem)?.Tag as string ?? "");
        return box;
    }

    /// <summary>A title that reveals its children; the expansion lives in the store and arrives as a flag.</summary>
    UIElement Disclosure(Scope scope, Node node)
    {
        var expander = new Expander { Header = node.Text("title"), IsExpanded = node.Flag("expanded"), Content = Column(Children(scope, node)), HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Left };
        var emit = Emit(node);
        expander.Expanding += (_, _) => emit(true);
        expander.Collapsed += (_, _) => emit(false);
        return expander;
    }

    /// <summary>A dialog shown while the alert is visible; closing it sends one dismissal.</summary>
    UIElement Alert(Scope scope, Node node)
    {
        var anchor = new Border();
        if (node.Flag("visible"))
        {
            var emit = Emit(node);
            anchor.Loaded += async (_, _) =>
            {
                var dialog = new ContentDialog { Title = node.Text("title"), Content = node.Text("message"), CloseButtonText = "OK", XamlRoot = anchor.XamlRoot };
                await dialog.ShowAsync();
                emit(null);
            };
        }
        return anchor;
    }

    /// <summary>Selectable rows under their group headings; clicking one sends its key, and the chosen row shows selected.</summary>
    UIElement Rows(Scope scope, Node node)
    {
        var list = new ListView { SelectionMode = ListViewSelectionMode.Single, IsItemClickEnabled = true };
        var group = "";
        foreach (var row in node.Items("rows"))
        {
            group = Grouped(list, group, row.Text("group"));
            list.Items.Add(Row(scope, row));
        }
        list.SelectedItem = Tagged<ListViewItem>(list.Items, node.Text("selected"));
        var emit = Emit(node);
        Action<object> click = clicked => emit(Key(clicked));
        // The list carries the name its handler is kept under. Its managed wrapper is no key: nothing
        // holds the wrapper once the list is drawn, so it can be collected and remade for the same list.
        var handle = $"{scope.Region}#{node.Number("event")}";
        list.Tag = handle;
        clicks[handle] = click;
        list.ItemClick += (_, args) => click(args.ClickedItem);
        return list;
    }

    /// <summary>Chooses a row through the handler its list's clicks run; false when it is in no drawn list.</summary>
    public bool Choose(ListViewItem row)
    {
        if (ItemsControl.ItemsControlFromItemContainer(row) is ListView { Tag: string handle } && clicks.TryGetValue(handle, out var click))
        {
            click(row);
            return true;
        }
        return false;
    }

    static string Grouped(ListView list, string current, string group)
    {
        if (group.Length > 0 && group != current)
        {
            list.Items.Add(new ListViewHeaderItem { Content = group });
        }
        return group;
    }

    ListViewItem Row(Scope scope, Node row)
    {
        var cell = new Grid { ColumnSpacing = Gap(2), Tag = row.Text("key") };
        var content = Draw(scope, row.Member("child"));
        Place(cell, Glyph(row.Text("symbol")), across: true, GridLength.Auto);
        Place(cell, content, across: true, Rest);
        return new ListViewItem { Content = cell, Tag = row.Text("key"), HorizontalContentAlignment = HorizontalAlignment.Stretch, Padding = new Thickness(Gap(1)) };
    }

    // The item whose tag is `key`, if one is.
    static object? Tagged<T>(ItemCollection items, string key) where T : FrameworkElement =>
        items.OfType<T>().FirstOrDefault(item => (string)item.Tag == key);

    static string Key(object clicked) => clicked switch
    {
        FrameworkElement { Tag: string key } => key,
        _ => "",
    };

    /// <summary>The page's toolbar as a command bar: buttons and toggles become app bar commands, anything else is hosted as is.</summary>
    CommandBar Toolbar(Scope scope, Node node)
    {
        var bar = new CommandBar { DefaultLabelPosition = CommandBarDefaultLabelPosition.Right, HorizontalAlignment = HorizontalAlignment.Right, Background = null };
        foreach (var item in node.Items("toolbar"))
        {
            bar.PrimaryCommands.Add(Command(scope, item));
        }
        return bar;
    }

    ICommandBarElement Command(Scope scope, Node item)
    {
        var emit = Emit(item);
        switch (item.Text("kind"))
        {
            case "button":
                var button = new AppBarButton { Label = item.Text("label"), Icon = IconOf(item.Text("symbol")), IsEnabled = item.Flag("enabled") };
                button.Click += (_, _) => emit(null);
                return button;
            case "toggle":
                var toggle = new AppBarToggleButton { Label = item.Text("label"), IsChecked = item.Flag("on") };
                toggle.Checked += (_, _) => emit(true);
                toggle.Unchecked += (_, _) => emit(false);
                return toggle;
            default:
                return new AppBarElementContainer { Content = Draw(scope, item), VerticalContentAlignment = VerticalAlignment.Center };
        }
    }
}
