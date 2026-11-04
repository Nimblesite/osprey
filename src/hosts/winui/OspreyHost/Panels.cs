using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using static Osprey.Host.Envelopes;

namespace Osprey.Host.WinUI;

/// <summary>Panes that hold state in the store: asides with their splitter, and tabs [UI-COMPONENTS-SPLITTER].</summary>
sealed partial class Renderer
{
    const double SplitterWidth = 6;

    /// <summary>A pane with a column beside it; dragging the splitter previews the width and sends it once released.</summary>
    UIElement Aside(Scope scope, JsonElement node)
    {
        var leading = Text(node, "side") == "leading";
        var grid = new Grid();
        var column = new ColumnDefinition { Width = new GridLength(Number(node, "width")), MinWidth = Number(node, "least"), MaxWidth = Number(node, "most") };
        var pane = new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star), MinWidth = Number(node, "keep") };
        var splitter = Splitter(column, leading, Emit(scope, node));
        var parts = leading
            ? new[] { (column, Draw(scope, Member(node, "aside"))), (new ColumnDefinition { Width = GridLength.Auto }, splitter), (pane, Draw(scope, Member(node, "pane"))) }
            : new[] { (pane, Draw(scope, Member(node, "pane"))), (new ColumnDefinition { Width = GridLength.Auto }, splitter), (column, Draw(scope, Member(node, "aside"))) };
        foreach (var (definition, element) in parts)
        {
            Grid.SetColumn((FrameworkElement)element, grid.ColumnDefinitions.Count);
            grid.ColumnDefinitions.Add(definition);
            grid.Children.Add(element);
        }
        return grid;
    }

    static UIElement Splitter(ColumnDefinition column, bool leading, Action<object?> emit)
    {
        var bar = new Border { Width = SplitterWidth, Background = Brush("DividerStrokeColorDefaultBrush") };
        var start = 0.0;
        var width = 0.0;
        bar.PointerPressed += (_, args) => { bar.CapturePointer(args.Pointer); start = args.GetCurrentPoint(null).Position.X; width = column.ActualWidth; };
        bar.PointerMoved += (_, args) =>
        {
            if (bar.PointerCaptures?.Count > 0)
            {
                var moved = args.GetCurrentPoint(null).Position.X - start;
                column.Width = new GridLength(Math.Clamp(width + (leading ? moved : -moved), column.MinWidth, column.MaxWidth));
            }
        };
        bar.PointerReleased += (_, args) => { bar.ReleasePointerCapture(args.Pointer); emit((long)Math.Round(column.ActualWidth)); };
        return bar;
    }

    /// <summary>Tabs as a selector bar over the page of the selected tab; choosing one sends its key.</summary>
    UIElement Tabs(Scope scope, JsonElement node)
    {
        var bar = new SelectorBar();
        var tabs = Items(node, "tabs", tab => tab);
        var selected = Text(node, "selected");
        foreach (var tab in tabs)
        {
            bar.Items.Add(new SelectorBarItem { Text = Text(tab, "label"), Icon = IconOf(Text(tab, "symbol")), Tag = Text(tab, "key"), IsSelected = Text(tab, "key") == selected });
        }
        var emit = Emit(scope, node);
        bar.SelectionChanged += (_, _) => emit(bar.SelectedItem?.Tag as string ?? "");
        var page = tabs.FirstOrDefault(tab => Text(tab, "key") == selected);
        var content = page.ValueKind == JsonValueKind.Object ? Draw(scope, Member(page, "child")) : new Border();
        return Column([bar, content]);
    }
}
