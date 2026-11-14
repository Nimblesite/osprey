using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Osprey.Host.WinUI;

/// <summary>Structure nodes: stacks, sections, pages, splits, layers, menus, asides and tabs [UI-COMPONENTS].</summary>
sealed partial class Renderer
{
    // Kinds that take the room left over in a column: lists, states and pages that scroll or lay
    // themselves out in the room they are given, and scenes, which scale to it.
    static readonly HashSet<string> Fillers = ["rows", "empty", "aside", "scene", "screen", "split"];
    const int Nesting = 32;
    internal static readonly GridLength Rest = new(1, GridUnitType.Star);

    /// <summary>
    /// Children in a row or column; a spacer takes the room left over, and in a column so does a
    /// child that fills, such as rows or a scene, which then scrolls or scales on its own.
    /// </summary>
    UIElement Stack(Scope scope, Node node)
    {
        var across = node.Text("axis") == "horizontal";
        var grid = new Grid { RowSpacing = Gap(1), ColumnSpacing = Gap(1) };
        foreach (var child in node.Items("children"))
        {
            var element = Draw(scope, child);
            var room = child.Text("kind") == "spacer" ? Rest : across ? GridLength.Auto : Room(child);
            Remember(child, Place(grid, element, across, room));
            if (across)
            {
                ((FrameworkElement)element).VerticalAlignment = VerticalAlignment.Center;
            }
        }
        return grid;
    }

    /// <summary>
    /// Whether a node takes the room left over in a column: a kind that fills, a column holding one,
    /// or a slot whose region fills now. A slot is settled again each time its region is drawn.
    /// </summary>
    bool Fills(Node node, int depth = 0) => depth < Nesting && node.Text("kind") switch
    {
        "slot" => regions.Find(node.Text("key")) is { } region && Fills(region.Node, depth + 1),
        "stack" => node.Text("axis") != "horizontal" && node.Items("children").Any(child => Fills(child, depth + 1)),
        "themed" => node.Has("body") && Fills(node.Member("body"), depth + 1),
        var kind => Fillers.Contains(kind),
    };

    GridLength Room(Node node) => Fills(node) ? Rest : GridLength.Auto;

    /// <summary>
    /// The body with a theme of its own [UI-COMPONENTS], [UI-MACHINE-SKINS]: on its <c>background</c>
    /// token, in its <c>text</c> ink, which the body's words inherit, and with the light or dark
    /// appearance that suits that background, so the stock controls inside stay legible on it.
    /// </summary>
    UIElement Themed(Scope scope, Node node)
    {
        if (!node.Has("theme") || !node.Member("theme").IsRecord || !node.Has("body"))
        {
            return Refused("A themed node needs a theme and a body");
        }
        var body = node.Member("body");
        var theme = Theme.Of(node.Member("theme"));
        var dark = Application.Current.RequestedTheme == ApplicationTheme.Dark;
        var fill = Token(theme, "background", dark);
        var inner = new ContentControl
        {
            Content = Draw(scope, body),
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            VerticalContentAlignment = VerticalAlignment.Stretch,
            RequestedTheme = fill is { } colour && Luminance(colour) < 0.5 ? ElementTheme.Dark : ElementTheme.Light,
        };
        if (Token(theme, "text", dark) is { } ink)
        {
            inner.Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(ink);
        }
        var border = new Border { Child = inner, Padding = new Thickness(Gap(1) * 3) };
        if (fill is { } background)
        {
            border.Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(background);
        }
        return border;
    }

    /// <summary>The colour of the theme's token called <paramref name="name"/> in this appearance, or null when it has none.</summary>
    static Windows.UI.Color? Token(Theme? theme, string name, bool dark) =>
        theme?.Tokens.Where(token => token.Name == name)
            .Select(token => Hex(dark ? token.Dark : token.Light)).FirstOrDefault(colour => colour is not null);

    static Windows.UI.Color? Hex(string text) =>
        text is ['#', .. var digits] && digits.Length == 6 && uint.TryParse(digits, System.Globalization.NumberStyles.HexNumber, null, out var rgb)
            ? Windows.UI.Color.FromArgb(255, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb)
            : null;

    static double Luminance(Windows.UI.Color colour) => (0.2126 * colour.R + 0.7152 * colour.G + 0.0722 * colour.B) / 255;

    // A slot's row in a column, kept so that drawing the slot's region settles the row again.
    void Remember(Node child, RowDefinition? row)
    {
        if (child.Text("kind") == "slot" && row is not null)
        {
            rooms[child.Text("key")] = row;
        }
    }

    /// <summary>Adds a column or row of <paramref name="room"/> to the grid for the element; returns the row, if it was one.</summary>
    internal static RowDefinition? Place(Grid grid, UIElement element, bool across, GridLength room)
    {
        RowDefinition? row = null;
        if (across)
        {
            Grid.SetColumn((FrameworkElement)element, grid.ColumnDefinitions.Count);
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = room });
        }
        else
        {
            Grid.SetRow((FrameworkElement)element, grid.RowDefinitions.Count);
            row = new RowDefinition { Height = room };
            grid.RowDefinitions.Add(row);
        }
        grid.Children.Add(element);
        return row;
    }

    /// <summary>A card headed by its title as an eyebrow, with its footer as explanatory text.</summary>
    UIElement Section(Scope scope, Node node)
    {
        var parts = new List<UIElement>();
        if (node.Text("title") is { Length: > 0 } title)
        {
            parts.Add(Words(title, "headline"));
        }
        parts.AddRange(Children(scope, node));
        if (node.Text("footer") is { Length: > 0 } footer)
        {
            parts.Add(Words(footer, "caption", "secondary"));
        }
        return Card(Column(parts));
    }

    UIElement Card(UIElement content) => new Border
    {
        Background = Brush("CardBackgroundFillColorDefaultBrush"),
        BorderBrush = Brush("CardStrokeColorDefaultBrush"),
        BorderThickness = new Thickness(1),
        CornerRadius = new CornerRadius(8),
        Padding = new Thickness(Gap(3)),
        Child = content,
    };

    /// <summary>A page: its title beside a command bar of its toolbar, then its children, scrolling unless one fills.</summary>
    UIElement Screen(Scope scope, Node node)
    {
        var page = new Grid { Padding = new Thickness(Gap(4), Gap(3), Gap(4), Gap(3)), RowSpacing = Gap(2) };
        var body = Body(scope, node.Items("children"));
        Place(page, Heading(scope, node), across: false, GridLength.Auto);
        Place(page, body, across: false, Rest);
        return page;
    }

    UIElement Heading(Scope scope, Node node)
    {
        // The title above its commands, as a Windows 11 page lays them out, so neither crowds the other out.
        var titles = Column([OneLine(Words(node.Text("title"), "title")), OneLine(Words(node.Text("subtitle"), "body", "secondary"))], 0);
        return Column([titles, Toolbar(scope, node)], 1);
    }

    // A heading keeps to one line, as File Explorer's does, trimmed with an ellipsis when the window
    // is too narrow for it, and shows the whole text in its tooltip.
    static TextBlock OneLine(TextBlock words)
    {
        words.TextWrapping = TextWrapping.NoWrap;
        words.TextTrimming = TextTrimming.CharacterEllipsis;
        ToolTipService.SetToolTip(words, words.Text);
        return words;
    }

    // A page scrolls as a whole unless a child fills, or is a slot that may fill once its region is drawn.
    UIElement Body(Scope scope, IReadOnlyList<Node> children)
    {
        if (!children.Any(child => child.Text("kind") == "slot" || Fills(child)))
        {
            return new ScrollViewer { Content = Column(children.Select(child => Draw(scope, child))) };
        }
        var grid = new Grid { RowSpacing = Gap(2) };
        foreach (var child in children)
        {
            Remember(child, Place(grid, Draw(scope, child), false, Room(child)));
        }
        return grid;
    }

    /// <summary>Navigation columns: the first pane is the sidebar on the window's backdrop; the rest are content layers.</summary>
    UIElement Split(Scope scope, Node node)
    {
        var grid = new Grid();
        var panes = node.Items("children");
        for (var index = 0; index < panes.Count; index++)
        {
            var width = index == 0 ? new GridLength(280) : new GridLength(1, GridUnitType.Star);
            Place(grid, index == 0 ? Draw(scope, panes[index]) : Layered(Draw(scope, panes[index])), true, width);
        }
        return grid;
    }

    UIElement Layered(UIElement content) => new Border
    {
        Background = Brush("LayerFillColorDefaultBrush"),
        BorderBrush = Brush("CardStrokeColorDefaultBrush"),
        BorderThickness = new Thickness(1, 1, 0, 0),
        CornerRadius = new CornerRadius(8, 0, 0, 0),
        Child = content,
    };

    /// <summary>Children drawn over one another.</summary>
    UIElement Layer(Scope scope, Node node)
    {
        var grid = new Grid();
        foreach (var child in Children(scope, node))
        {
            grid.Children.Add(child);
        }
        return grid;
    }

    /// <summary>Content with its secondary actions in a context flyout, reached by right-click, touch hold or the menu key.</summary>
    UIElement ContextMenu(Scope scope, Node node)
    {
        var body = Draw(scope, node.Member("body"));
        var menu = new MenuFlyout();
        foreach (var item in node.Items("items"))
        {
            menu.Items.Add(MenuItem(scope, item));
        }
        body.ContextFlyout = menu;
        return body;
    }

    MenuFlyoutItemBase MenuItem(Scope scope, Node item)
    {
        if (item.Text("kind") != "button")
        {
            Refused($"A context menu holds buttons, not {item.Text("kind")} nodes");
            return new MenuFlyoutSeparator();
        }
        var entry = new MenuFlyoutItem { Text = item.Text("label"), Icon = IconOf(item.Text("symbol")), IsEnabled = item.Flag("enabled") };
        var emit = Emit(item);
        entry.Click += (_, _) => emit(null);
        return entry;
    }
}
