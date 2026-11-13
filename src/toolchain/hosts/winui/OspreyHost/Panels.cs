using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Osprey.Host.WinUI;

/// <summary>Panes that hold state in the store: asides with their splitter, and tabs [UI-COMPONENTS-SPLITTER].</summary>
sealed partial class Renderer
{
    const double SplitterWidth = 6;

    /// <summary>A pane with a column beside it; dragging the splitter previews the width and sends it once released.</summary>
    UIElement Aside(Scope scope, Node node)
    {
        var leading = node.Text("side") == "leading";
        var grid = new Grid();
        var column = new ColumnDefinition { Width = new GridLength(node.Number("width")), MinWidth = node.Number("least"), MaxWidth = node.Number("most") };
        var pane = new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star), MinWidth = node.Number("keep") };
        var splitter = Splitter(column, leading, Emit(node));
        var parts = leading
            ? new[] { (column, Draw(scope, node.Member("aside"))), (new ColumnDefinition { Width = GridLength.Auto }, splitter), (pane, Draw(scope, node.Member("pane"))) }
            : new[] { (pane, Draw(scope, node.Member("pane"))), (new ColumnDefinition { Width = GridLength.Auto }, splitter), (column, Draw(scope, node.Member("aside"))) };
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
    UIElement Tabs(Scope scope, Node node)
    {
        var bar = new SelectorBar();
        var tabs = node.Items("tabs");
        var selected = node.Text("selected");
        foreach (var tab in tabs)
        {
            bar.Items.Add(new SelectorBarItem { Text = tab.Text("label"), Icon = IconOf(tab.Text("symbol")), Tag = tab.Text("key"), IsSelected = tab.Text("key") == selected });
        }
        var emit = Emit(node);
        bar.SelectionChanged += (_, _) => emit(bar.SelectedItem?.Tag as string ?? "");
        var page = tabs.FirstOrDefault(tab => tab.Text("key") == selected);
        var content = page.IsRecord ? Draw(scope, page.Member("child")) : new Border();
        return Column([bar, content]);
    }
}
