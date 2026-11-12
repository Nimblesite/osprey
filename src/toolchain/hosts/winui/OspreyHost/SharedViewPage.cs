using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Osprey.Host.WinUI;

/// <summary>The restyle delegate's shape: a row is restyled from the whole view, so a picker can rebuild its items from its children.</summary>
delegate void Restyling(Rows rows, int index);

/// <summary>
/// The shared view [NATIVE-SHM-VIEW]: the rows an application wrote into the shared frame, drawn
/// as real WinUI controls. After every call the page leases the frame's rows and their text, reads
/// both in place through <see cref="Rows"/> — the one reader that makes every row safe
/// [NATIVE-SHM-SAFETY] — and updates the controls it already drew, row by row, so an entry keeps
/// its focus and caret. Every colour, size, weight and space comes from the rows; the host holds
/// no style of its own. A refused call keeps the view the page already shows, and a sheet floats
/// above everything in the page's overlay.
/// </summary>
sealed partial class SharedViewPage(Library library, Border page, Action<IReadOnlyList<string>> report)
{
    static readonly GridLength Auto = GridLength.Auto;

    readonly List<Seat?> controls = [];
    readonly Grid board = new(); // the view's root, with the sheet overlay over it
    readonly Grid overlay = new(); // the rows' sheets float here, above the view
    Run<NativeView>? rows;
    Run<byte>? text;
    bool applying;

    /// <summary>Draws the first view, from the frame <see cref="Library.StartView"/> sealed.</summary>
    public void Begin()
    {
        page.Child = board;
        Show(library.StartView());
    }

    // One tap or edit: an integer and the words cross the boundary, and the next view is drawn or kept.
    void Send(long action, string words) => Show(library.EventView(action, words));

    void Show(long status)
    {
        if (status != 0)
        {
            report([$"The application refused its call: error {status}"]);
            return;
        }
        try
        {
            Swapped();
        }
        catch (HostException problem)
        {
            report([problem.Message]);
        }
    }

    // Leases the new view before the previous one is released, then draws its rows in place.
    void Swapped()
    {
        var nextRows = library.Bridge.Take<NativeView>(Bridge.ViewsRun);
        var nextText = library.Bridge.Take<byte>(Bridge.TextRun);
        try
        {
            applying = true;
            Drawn(new Rows(nextRows.Bytes, nextText.Bytes));
        }
        catch
        {
            // A failed draw leaves the new leases with nothing reading them, and the old view showing.
            nextRows.Dispose();
            nextText.Dispose();
            throw;
        }
        finally
        {
            applying = false;
        }
        rows?.Dispose();
        text?.Dispose();
        (rows, text) = (nextRows, nextText);
    }

    void Drawn(Rows next)
    {
        if (controls.Count == next.Count && Shaped(next))
        {
            Updated(next);
            return;
        }
        Built(next);
    }

    // Whether the next view seats the same controls the same way, so the old ones can be restyled
    // in place. A row that comes back drawable after being skipped, or the other kind of field,
    // rebuilds the tree.
    bool Shaped(Rows next)
    {
        for (var index = 0; index < next.Count; index++)
        {
            var seat = controls[index];
            if (next.Draws(index))
            {
                if (seat is null || !Same(seat, next.At(index)))
                {
                    return false;
                }
            }
            else if (seat is not null)
            {
                return false;
            }
        }
        return true;
    }

    static bool Same(Seat seat, SafeRow row) => seat.Kind == row.Kind && seat.Parent == row.Parent && seat.Secure == row.Secure;

    // Restyles the controls already on screen, row by row, so focus and caret never move.
    void Updated(Rows next)
    {
        for (var index = 0; index < next.Count; index++)
        {
            if (controls[index] is { } seat)
            {
                seat.Action = next.At(index).Action;
                seat.Restyle?.Invoke(next, index);
            }
        }
    }

    // Builds every control fresh: one seat per row index, the tree the rows name.
    void Built(Rows next)
    {
        var seats = new Seat?[next.Count];
        overlay.Children.Clear();
        Seat? root = null;
        for (var index = 0; index < next.Count; index++)
        {
            var seat = next.Draws(index) ? Seated(next, index) : null;
            seats[index] = seat;
            if (seat is null)
            {
                continue;
            }
            seat.Parent = next.At(index).Parent;
            if (seat.Floating)
            {
                continue; // a sheet already floats in the overlay
            }
            var parent = seat.Parent;
            if (parent == Rows.None)
            {
                root ??= seat; // the first root draws; the view shows one
            }
            else
            {
                Placed(seats[parent], index, seat, next);
            }
        }
        controls.Clear();
        controls.AddRange(seats);
        board.Children.Clear();
        if (root is { } top)
        {
            board.Children.Add(top.Element);
        }
        board.Children.Add(overlay);
    }

    // Seats one drawn child in its parent: a picker keeps its options as items, a menu shows only
    // its first child, a layer shares its one cell, and the rest take a grid slot sized by the row.
    void Placed(Seat? parent, int index, Seat child, Rows rows)
    {
        if (parent?.Panel is not { } pile)
        {
            return; // nothing to sit in: the row draws nothing rather than take the host down
        }
        if (parent.Kind == RowKind.Picker || parent.Kind == RowKind.Menu && parent.Shown++ > 0)
        {
            return; // a picker's options are its items; a menu's other children are its context menu
        }
        if (parent.Kind == RowKind.Layer)
        {
            pile.Children.Add(child.Element); // every layer child shares the one cell; the last is in front
            return;
        }
        Roomed(parent, pile, child, rows.At(index));
    }

    // One child's slot in its parent's grid: a flexible child takes its share of the room left
    // over, a gap takes one share by definition, and every other child takes what it needs.
    static void Roomed(Seat parent, Grid pile, Seat child, SafeRow row)
    {
        var share = row.Grow > 0 ? row.Grow : row.Kind == RowKind.Gap ? 1 : 0;
        var room = share > 0 ? new GridLength(share, GridUnitType.Star) : Auto;
        if (parent.Across)
        {
            Grid.SetColumn(child.Element, pile.ColumnDefinitions.Count);
            pile.ColumnDefinitions.Add(new ColumnDefinition { Width = room });
        }
        else
        {
            Grid.SetRow(child.Element, pile.RowDefinitions.Count);
            pile.RowDefinitions.Add(new RowDefinition { Height = room });
        }
        pile.Children.Add(child.Element);
    }

    /// <summary>One drawn control: what sits in its parent, how a later row restyles it, and the event it delivers.</summary>
    sealed class Seat(FrameworkElement element, RowKind kind)
    {
        public FrameworkElement Element { get; } = element;

        public RowKind Kind { get; } = kind;

        /// <summary>The row it was seated in, kept so the page can see the tree is unchanged.</summary>
        public int Parent { get; set; }

        public Grid? Panel { get; init; }

        public bool Across { get; init; }

        /// <summary>A sheet floats in the page's overlay instead of sitting in its parent.</summary>
        public bool Floating { get; init; }

        /// <summary>A secure field is another control: the view rebuilds when this changes.</summary>
        public bool Secure { get; init; }

        /// <summary>How many of a menu's children were shown in its panel; the rest are its items.</summary>
        public int Shown { get; set; }

        public Restyling? Restyle { get; set; }

        public long Action { get; set; }

        /// <summary>Whether an entry delivers its words only when committed with Return.</summary>
        public bool Commit { get; set; }
    }
}
