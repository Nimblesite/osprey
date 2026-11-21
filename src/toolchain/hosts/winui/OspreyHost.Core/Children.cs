using System.Diagnostics;
using System.Text;

namespace Osprey.Host;

/// <summary>
/// One long-lived child process [UI-EFFECTS-PROCESS], such as an MCP stdio server: standard input
/// stays open, standard output and error gather in separate buffers that keep at most their last
/// <c>cap</c> bytes, and a read takes what has arrived so far within a byte limit and a deadline,
/// never waiting longer. Every failure is a typed message for the application.
/// </summary>
public sealed class Child
{
    /// <summary>How long a child's last output may take to arrive after it exits.</summary>
    static readonly TimeSpan DrainGrace = TimeSpan.FromSeconds(1);

    /// <summary>How long a stopped child has to end before it is killed.</summary>
    public static readonly TimeSpan StopGrace = TimeSpan.FromSeconds(2);

    /// <summary>The longest UTF-8 character, so a piece of any limit can hold a whole one.</summary>
    const int SmallestPiece = 4;
    const int Block = 8192;

    readonly Process process;
    readonly int cap;
    readonly Lock gate = new();
    readonly SemaphoreSlim writing = new(1, 1);
    readonly List<byte>[] buffers = [[], []];
    readonly Task[] drains;
    bool ended;
    bool inputClosed;
    int status;

    Child(Process process, int cap)
    {
        this.process = process;
        this.cap = cap;
        drains = [Drain(process.StandardOutput.BaseStream, 0), Drain(process.StandardError.BaseStream, 1)];
        process.EnableRaisingEvents = true;
        process.Exited += (_, _) => _ = Task.Run(Exited);
        if (process.HasExited)
        {
            _ = Task.Run(Exited);
        }
    }

    /// <summary>The child's process id.</summary>
    public int Id => process.Id;

    /// <summary>True until the child has exited and its output has drained.</summary>
    public bool Running
    {
        get
        {
            lock (gate)
            {
                return !ended;
            }
        }
    }

    /// <summary>Starts <paramref name="command"/>, found as an absolute path or on the environment's PATH.</summary>
    public static Child Start(string command, IEnumerable<string> arguments, IReadOnlyList<(string Name, string Value)> environment, int cap)
    {
        var start = new ProcessStartInfo
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            StandardInputEncoding = new UTF8Encoding(false),
        };
        foreach (var (name, value) in environment)
        {
            start.Environment[name] = value;
        }
        start.FileName = Resolve(command, start.Environment.TryGetValue("PATH", out var path) ? path ?? "" : "");
        foreach (var argument in arguments)
        {
            start.ArgumentList.Add(argument);
        }
        return new Child(Process.Start(start) ?? throw new HostException($"{command} did not start"), cap);
    }

    /// <summary>
    /// <paramref name="command"/> as a file that exists: an absolute path as it is, or a name looked up
    /// in each folder of <paramref name="path"/>: as given, or on Windows with each executable extension.
    /// </summary>
    public static string Resolve(string command, string path)
    {
        if (Path.IsPathFullyQualified(command))
        {
            return File.Exists(command) ? command : throw new HostException($"Command not found: {command}");
        }
        // Windows never runs a file without an executable extension, such as MSYS2's bash script `cmd`.
        string[] extensions = Path.HasExtension(command) || !OperatingSystem.IsWindows() ? [""] : [".exe", ".cmd", ".bat", ".com"];
        var found = path.Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries)
            .SelectMany(folder => extensions.Select(extension => Path.Combine(folder.Trim('"'), command + extension)))
            .FirstOrDefault(candidate => Path.IsPathFullyQualified(candidate) && File.Exists(candidate));
        return found ?? throw new HostException($"Command not found: {command}");
    }

    async Task Drain(Stream stream, int slot)
    {
        var block = new byte[Block];
        try
        {
            int read;
            while ((read = await stream.ReadAsync(block)) > 0)
            {
                Append(slot, block.AsSpan(0, read));
            }
        }
        catch (Exception closed) when (closed is IOException or ObjectDisposedException)
        {
            // The pipe closed under the read: the child is gone and nothing more will come.
        }
    }

    void Append(int slot, ReadOnlySpan<byte> bytes)
    {
        lock (gate)
        {
            var buffer = buffers[slot];
            buffer.AddRange(bytes);
            if (buffer.Count > cap)
            {
                buffer.RemoveRange(0, buffer.Count - cap);
            }
        }
    }

    async Task Exited()
    {
        await Task.WhenAny(Task.WhenAll(drains), Task.Delay(DrainGrace));
        lock (gate)
        {
            ended = true;
            status = process.ExitCode;
        }
    }

    /// <summary>Writes <paramref name="text"/> to the child's standard input and answers how many bytes went.</summary>
    public async Task<int> Write(string text)
    {
        var bytes = Encoding.UTF8.GetBytes(text);
        await writing.WaitAsync();
        try
        {
            if (!Running || inputClosed)
            {
                throw new HostException($"The process {Id} has exited");
            }
            await process.StandardInput.BaseStream.WriteAsync(bytes);
            await process.StandardInput.BaseStream.FlushAsync();
            return bytes.Length;
        }
        catch (IOException broken)
        {
            throw new HostException($"The process {Id} stopped reading: {broken.Message}");
        }
        finally
        {
            writing.Release();
        }
    }

    /// <summary>The output so far, whole characters up to <paramref name="limit"/> bytes of each, waiting at most <paramref name="timeout"/>.</summary>
    public async Task<Reading> Read(int limit, TimeSpan timeout)
    {
        var deadline = DateTime.UtcNow + timeout;
        while (true)
        {
            var piece = Take(Math.Max(limit, SmallestPiece));
            if (piece.Output.Length > 0 || piece.Errors.Length > 0 || piece.Exited || DateTime.UtcNow >= deadline)
            {
                return piece;
            }
            await Task.Delay(10);
        }
    }

    Reading Take(int limit)
    {
        lock (gate)
        {
            var output = Taken(buffers[0], limit);
            var errors = Taken(buffers[1], limit);
            // `exited` is the last thing a reader sees: after it, nothing more will come.
            return new Reading(output, errors, ended && buffers[0].Count == 0 && buffers[1].Count == 0, status);
        }
    }

    string Taken(List<byte> buffer, int limit)
    {
        var size = Whole(buffer, limit, ended);
        var text = Encoding.UTF8.GetString(buffer.GetRange(0, size).ToArray());
        buffer.RemoveRange(0, size);
        return text;
    }

    /// <summary>
    /// How many leading bytes of <paramref name="bytes"/>, at most <paramref name="limit"/>, end on a
    /// character boundary. Once the child has ended nothing can complete a cut character at the very
    /// end, so that passes as it is.
    /// </summary>
    public static int Whole(IReadOnlyList<byte> bytes, int limit, bool finished)
    {
        var size = Math.Min(limit, bytes.Count);
        if (size == 0 || finished && size == bytes.Count)
        {
            return size;
        }
        var last = size - 1;
        while (last > 0 && size - last < SmallestPiece && (bytes[last] & 0xC0) == 0x80)
        {
            last--;
        }
        var lead = bytes[last];
        var length = lead >= 0xF0 ? 4 : lead >= 0xE0 ? 3 : lead >= 0xC0 ? 2 : 1;
        return size - last < length ? last : size;
    }

    /// <summary>
    /// Asks the child to end by closing its input, as the MCP specification's stdio shutdown begins,
    /// and kills it, with anything it started, if it is still running after <see cref="StopGrace"/>.
    /// </summary>
    public void Stop()
    {
        _ = Task.Run(async () =>
        {
            await writing.WaitAsync();
            try
            {
                inputClosed = true;
                process.StandardInput.Close();
            }
            catch (IOException)
            {
                // Its input was already gone.
            }
            finally
            {
                writing.Release();
            }
            await Task.Delay(StopGrace);
            Kill();
        });
    }

    /// <summary>Kills the child and its descendants now, if it is still running.</summary>
    public void Kill()
    {
        try
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }
        }
        catch (Exception gone) when (gone is InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            // It ended between the check and the kill.
        }
    }
}

/// <summary>What one read took: whole characters of each stream, whether the child has finished, and its exit status.</summary>
public readonly record struct Reading(string Output, string Errors, bool Exited, int Status);
