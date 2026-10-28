using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using static Osprey.Host.Envelopes;

namespace Osprey.Host.WinUI;

/// <summary>Structure nodes: stacks, sections, pages, splits, layers, menus, asides and tabs [UI-COMPONENTS].</summary>
sealed partial class Renderer
{
    // Kinds that take the room left over in a column: lists, states and pages that scroll or lay
    // themselves out in the room they are given, and scenes, which scale to it.
    static readonly HashSet<string> Fillers = ["rows", "empty", "aside", "scene", "screen", "split"];
    const int Nesting = 32;
    static readonly GridLength Rest = new(1, GridUnitType.Star);

    /// <summary>
    /// Children in a row or column; a spacer takes the room left over, and in a column so does a
    /// child that fills, such as rows or a scene, which then scrolls or scales on its own.
    /// </summary>
    UIElement Stack(Scope scope, JsonElement node)
    {
        var across = Text(node, "axis") == "horizontal";
        var grid = new Grid { RowSpacing = Gap(1), ColumnSpacing = Gap(1) };
        foreach (var child in Items(node, "children", item => item))
        {
            var element = Draw(scope, child);
            var room = Text(child, "kind") == "spacer" ? Rest : across ? GridLength.Auto : Room(child);
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
    bool Fills(JsonElement node, int depth = 0) => depth < Nesting && Text(node, "kind") switch
    {
        "slot" => regions.Find(Text(node, "key")) is { } region && Fills(region.Node, depth + 1),
        "stack" => Text(node, "axis") != "horizontal" && Items(node, "children", child => child).Any(child => Fills(child, depth + 1)),
        var kind => Fillers.Contains(kind),
    };

    GridLength Room(JsonElement node) => Fills(node) ? Rest : GridLength.Auto;

    // A slot's row in a column, kept so that drawing the slot's region settles the row again.
    void Remember(JsonElement child, RowDefinition? row)
    {
        if (Text(child, "kind") == "slot" && row is not null)
        {
            rooms[Text(child, "key")] = row;
        }
    }

    /// <summary>Adds a column or row of <paramref name="room"/> to the grid for the element; returns the row, if it was one.</summary>
    static RowDefinition? Place(Grid grid, UIElement element, bool across, GridLength room)
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
    UIElement Section(Scope scope, JsonElement node)
    {
        var parts = new List<UIElement>();
        if (Text(node, "title") is { Length: > 0 } title)
        {
            parts.Add(Words(title, "headline"));
        }
        parts.AddRange(Children(scope, node));
        if (Text(node, "footer") is { Length: > 0 } footer)
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
    UIElement Screen(Scope scope, JsonElement node)
    {
        var page = new Grid { Padding = new Thickness(Gap(4), Gap(3), Gap(4), Gap(3)), RowSpacing = Gap(2) };
        page.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        page.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var body = Body(scope, Items(node, "children", child => child));
        Grid.SetRow((FrameworkElement)body, 1);
        page.Children.Add(Heading(scope, node));
        page.Children.Add(body);
        return page;
    }

    UIElement Heading(Scope scope, JsonElement node)
    {
        var heading = new Grid { ColumnSpacing = Gap(3) };
        heading.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        heading.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var titles = Column([Words(Text(node, "title"), "title"), Words(Text(node, "subtitle"), "body", "secondary")], 0);
        var commands = Toolbar(scope, node);
        Grid.SetColumn(commands, 1);
        heading.Children.Add(titles);
        heading.Children.Add(commands);
        return heading;
    }

    // A page scrolls as a whole unless a child fills, or is a slot that may fill once its region is drawn.
    UIElement Body(Scope scope, IReadOnlyList<JsonElement> children)
    {
        if (!children.Any(child => Text(child, "kind") == "slot" || Fills(child)))
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
    UIElement Split(Scope scope, JsonElement node)
    {
        var grid = new Grid();
        var panes = Items(node, "children", pane => pane);
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
    UIElement Layer(Scope scope, JsonElement node)
    {
        var grid = new Grid();
        foreach (var child in Children(scope, node))
        {
            grid.Children.Add(child);
        }
        return grid;
    }

    /// <summary>Content with its secondary actions in a context flyout, reached by right-click, touch hold or the menu key.</summary>
    UIElement ContextMenu(Scope scope, JsonElement node)
    {
        var body = Draw(scope, Member(node, "body"));
        var menu = new MenuFlyout();
        foreach (var item in Items(node, "items", item => item))
        {
            menu.Items.Add(MenuItem(scope, item));
        }
        body.ContextFlyout = menu;
        return body;
    }

    MenuFlyoutItemBase MenuItem(Scope scope, JsonElement item)
    {
        if (Text(item, "kind") != "button")
        {
            Refused($"A context menu holds buttons, not {Text(item, "kind")} nodes");
            return new MenuFlyoutSeparator();
        }
        var entry = new MenuFlyoutItem { Text = Text(item, "label"), Icon = IconOf(Text(item, "symbol")), IsEnabled = Flag(item, "enabled") };
        var emit = Emit(scope, item);
        entry.Click += (_, _) => emit(null);
        return entry;
    }
}
