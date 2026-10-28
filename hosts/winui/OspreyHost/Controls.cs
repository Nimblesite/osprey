using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.System;
using static Osprey.Host.Envelopes;

namespace Osprey.Host.WinUI;

/// <summary>
/// Controls [UI-EVENTS]: each sends one value of its binding's kind under the ordinal the framework
/// issued. A control's handler is attached after its value is set, so drawing never sends an event.
/// </summary>
sealed partial class Renderer
{
    UIElement Button(Scope scope, JsonElement node)
    {
        var button = new Button { Content = Line(Glyph(Text(node, "symbol")), Words(Text(node, "label"))), IsEnabled = Flag(node, "enabled") };
        if (Text(node, "role") == "prominent")
        {
            button.Style = Style("AccentButtonStyle");
        }
        if (Text(node, "role") == "destructive")
        {
            button.Foreground = Tone("danger");
        }
        var emit = Emit(scope, node);
        button.Click += (_, _) => emit(null);
        return button;
    }

    /// <summary>A text box committing every edit, or only on Enter; a secure field is a password box.</summary>
    UIElement Field(Scope scope, JsonElement node)
    {
        var emit = Emit(scope, node);
        if (Flag(node, "secure"))
        {
            var secret = new PasswordBox { Header = Text(node, "label"), PlaceholderText = Text(node, "prompt"), Password = Text(node, "value") };
            secret.PasswordChanged += (_, _) => emit(secret.Password);
            return secret;
        }
        var box = new TextBox { Header = Text(node, "label"), PlaceholderText = Text(node, "prompt"), Text = Text(node, "value"), AcceptsReturn = Flag(node, "multiline") };
        if (Text(node, "commit") == "submit")
        {
            box.KeyDown += (_, args) => { if (args.Key == VirtualKey.Enter) { emit(box.Text); } };
        }
        else
        {
            box.TextChanged += (_, _) => emit(box.Text);
        }
        return box;
    }

    UIElement Toggle(Scope scope, JsonElement node)
    {
        var toggle = new ToggleSwitch { Header = Text(node, "label"), IsOn = Flag(node, "on") };
        var emit = Emit(scope, node);
        toggle.Toggled += (_, _) => emit(toggle.IsOn);
        return toggle;
    }

    UIElement Picker(Scope scope, JsonElement node)
    {
        var box = new ComboBox { Header = Text(node, "label") };
        foreach (var choice in Items(node, "choices", choice => choice))
        {
            box.Items.Add(new ComboBoxItem { Content = Text(choice, "label"), Tag = Text(choice, "key") });
        }
        box.SelectedItem = box.Items.OfType<ComboBoxItem>().FirstOrDefault(item => (string)item.Tag == Text(node, "selected"));
        var emit = Emit(scope, node);
        box.SelectionChanged += (_, _) => emit((box.SelectedItem as ComboBoxItem)?.Tag as string ?? "");
        return box;
    }

    /// <summary>A title that reveals its children; the expansion lives in the store and arrives as a flag.</summary>
    UIElement Disclosure(Scope scope, JsonElement node)
    {
        var expander = new Expander { Header = Text(node, "title"), IsExpanded = Flag(node, "expanded"), Content = Column(Children(scope, node)), HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Left };
        var emit = Emit(scope, node);
        expander.Expanding += (_, _) => emit(true);
        expander.Collapsed += (_, _) => emit(false);
        return expander;
    }

    /// <summary>A dialog shown while the alert is visible; closing it sends one dismissal.</summary>
    UIElement Alert(Scope scope, JsonElement node)
    {
        var anchor = new Border();
        if (Flag(node, "visible"))
        {
            var emit = Emit(scope, node);
            anchor.Loaded += async (_, _) =>
            {
                var dialog = new ContentDialog { Title = Text(node, "title"), Content = Text(node, "message"), CloseButtonText = "OK", XamlRoot = anchor.XamlRoot };
                await dialog.ShowAsync();
                emit(null);
            };
        }
        return anchor;
    }

    /// <summary>Selectable rows under their group headings; clicking one sends its key, and the chosen row shows selected.</summary>
    UIElement Rows(Scope scope, JsonElement node)
    {
        var list = new ListView { SelectionMode = ListViewSelectionMode.Single, IsItemClickEnabled = true };
        var group = "";
        foreach (var row in Items(node, "rows", row => row))
        {
            group = Grouped(list, group, Text(row, "group"));
            list.Items.Add(Row(scope, row));
        }
        list.SelectedItem = list.Items.OfType<ListViewItem>().FirstOrDefault(item => (string)item.Tag == Text(node, "selected"));
        var emit = Emit(scope, node);
        Action<object> click = clicked => emit(Key(clicked));
        // The list carries the name its handler is kept under. Its managed wrapper is no key: nothing
        // holds the wrapper once the list is drawn, so it can be collected and remade for the same list.
        var handle = $"{scope.Region}#{Envelopes.Number(node, "event")}";
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

    ListViewItem Row(Scope scope, JsonElement row)
    {
        var cell = new Grid { ColumnSpacing = Gap(2) };
        cell.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        cell.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var content = Draw(scope, Member(row, "child"));
        Grid.SetColumn((FrameworkElement)content, 1);
        cell.Children.Add(Glyph(Text(row, "symbol")));
        cell.Children.Add(content);
        cell.Tag = Text(row, "key");
        return new ListViewItem { Content = cell, Tag = Text(row, "key"), HorizontalContentAlignment = HorizontalAlignment.Stretch, Padding = new Thickness(Gap(1)) };
    }

    static string Key(object clicked) => clicked switch
    {
        FrameworkElement { Tag: string key } => key,
        _ => "",
    };

    /// <summary>The page's toolbar as a command bar: buttons and toggles become app bar commands, anything else is hosted as is.</summary>
    CommandBar Toolbar(Scope scope, JsonElement node)
    {
        var bar = new CommandBar { DefaultLabelPosition = CommandBarDefaultLabelPosition.Right, HorizontalAlignment = HorizontalAlignment.Right, Background = null };
        foreach (var item in Items(node, "toolbar", item => item))
        {
            bar.PrimaryCommands.Add(Command(scope, item));
        }
        return bar;
    }

    ICommandBarElement Command(Scope scope, JsonElement item)
    {
        var emit = Emit(scope, item);
        switch (Text(item, "kind"))
        {
            case "button":
                var button = new AppBarButton { Label = Text(item, "label"), Icon = IconOf(Text(item, "symbol")), IsEnabled = Flag(item, "enabled") };
                button.Click += (_, _) => emit(null);
                return button;
            case "toggle":
                var toggle = new AppBarToggleButton { Label = Text(item, "label"), IsChecked = Flag(item, "on") };
                toggle.Checked += (_, _) => emit(true);
                toggle.Unchecked += (_, _) => emit(false);
                return toggle;
            default:
                return new AppBarElementContainer { Content = Draw(scope, item), VerticalContentAlignment = VerticalAlignment.Center };
        }
    }
}
