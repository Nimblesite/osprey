namespace Osprey.Host;

/// <summary>
/// Every framework symbol [UI-COMPONENTS-SYMBOLS] as a Segoe Fluent Icons glyph, the icon set
/// Windows draws its own controls with. A name with no glyph is refused, never guessed.
/// </summary>
public static class Glyphs
{
    /// <summary>The glyph for each symbol name the framework sends.</summary>
    public static IReadOnlyDictionary<string, string> ByName { get; } = new Dictionary<string, string>
    {
        ["add"] = "",
        ["delete"] = "",
        ["dismiss"] = "",
        ["check"] = "",
        ["warning"] = "",
        ["info"] = "",
        ["question"] = "",
        ["search"] = "",
        ["settings"] = "",
        ["star"] = "",
        ["play"] = "",
        ["pause"] = "",
        ["stop"] = "",
        ["refresh"] = "",
        ["folder"] = "",
        ["document"] = "",
        ["package"] = "",
        ["inbox"] = "",
        ["globe"] = "",
        ["external"] = "",
        ["lock"] = "",
        ["shield"] = "",
        ["branch"] = "",
        ["tag"] = "",
        ["clock"] = "",
        ["person"] = "",
        ["cursor"] = "",
        ["chevron"] = "",
    };

    /// <summary>The glyph for <paramref name="name"/>; the empty name is no symbol.</summary>
    public static string? For(string name) =>
        name.Length == 0 ? "" : ByName.GetValueOrDefault(name);
}
