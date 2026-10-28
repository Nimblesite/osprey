using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using static Osprey.Host.Envelopes;

namespace Osprey.Host.WinUI;

/// <summary>Structure nodes: stacks, sections, pages, splits, layers, menus, asides and tabs [UI-COMPONENTS].</summary>
sealed partial class Renderer
{
    static readonly HashSet<string> Fillers = ["rows", "empty", "slot", "aside", "scene"];

    /// <summary>
    /// Children in a row or column; a spacer takes the room left over, and in a column so does a
    /// child that fills, such as rows or a region, which then scrolls on its own.
    /// </summary>
    UIElement Stack(Scope scope, JsonElement node)
    {
        var across = Text(node, "axis") == "horizontal";
        var grid = new Grid { RowSpacing = Gap(1), ColumnSpacing = Gap(1) };
        foreach (var child in Items(node, "children", item => item))
        {
            var kind = Text(child, "kind");
            var fills = kind == "spacer" || (!across && Fillers.Contains(kind));
            Place(grid, Draw(scope, child), across, fills ? new GridLength(1, GridUnitType.Star) : GridLength.Auto);
        }
        return grid;
    }

    static void Place(Grid grid, UIElement element, bool across, GridLength room)
    {
        if (across)
        {
            Grid.SetColumn((FrameworkElement)element, grid.ColumnDefinitions.Count);
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = room });
            ((FrameworkElement)element).VerticalAlignment = VerticalAlignment.Center;
        }
        else
        {
            Grid.SetRow((FrameworkElement)element, grid.RowDefinitions.Count);
            grid.RowDefinitions.Add(new RowDefinition { Height = room });
        }
        grid.Children.Add(element);
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

    UIElement Body(Scope scope, IReadOnlyList<JsonElement> children)
    {
        if (!children.Any(child => Fillers.Contains(Text(child, "kind"))))
        {
            return new ScrollViewer { Content = Column(children.Select(child => Draw(scope, child))) };
        }
        var grid = new Grid { RowSpacing = Gap(2) };
        foreach (var child in children)
        {
            var room = Fillers.Contains(Text(child, "kind")) ? new GridLength(1, GridUnitType.Star) : GridLength.Auto;
            Place(grid, Draw(scope, child), false, room);
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
