package org.ospreylang.host

import android.os.StatFs
import android.os.SystemClock
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import android.system.StructStat
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.NoSuchFileException
import java.nio.file.Paths

/** Implements [UI-EFFECTS-MEASURE] on the Android host with android.system.Os and java.nio.file:
 * each named folder's children are measured to the bottom — allocated bytes (`st_blocks * 512`),
 * a hard-linked file once per walk, links listed and never followed, the walk never leaving its
 * start volume — within an entry limit and a deadline, and the result goes to the slice Osprey
 * named. The adapter sorts, hides, formats and decides nothing; the application does all of
 * that. The completion itself is `null`. */
class MeasureAdapter : Adapter {
    override val kinds = setOf("measure-folders")

    override fun perform(ask: Ask, staged: StagedSlices, done: (Outcome) -> Unit) {
        try {
            val limit = ask.number(2).let {
                if (it < 0) throw IOException("Invalid measured entry limit") else minOf(it, MAX_ENTRIES).toInt()
            }
            val deadline = minOf(maxOf(ask.number(3), 0L), MAX_DEADLINE)
            val budget = Budget(deadline)
            val measured = ask.texts(1).map { one(it, limit, budget) }
            staged.write(ask.text(4), Doc.obj(listOf("folders" to Doc.arr(measured))))
            done(Outcome.Done("null"))
        } catch (cancelled: Interrupted) {
            done(Outcome.Refused("cancelled"))
        } catch (error: Exception) {
            done(Outcome.Refused(error.message ?: "The walk failed"))
        }
    }

    /** One named folder: its volume, and either the walk of its children or why it was not read. */
    private fun one(path: String, limit: Int, budget: Budget): String {
        val volume = volume(path)
        if (budget.expired) return opened(path, volume, "", false, truncated = true)
        val children = try {
            children(path)
        } catch (error: Exception) {
            val missing = error is NoSuchFileException
            return opened(path, volume, if (missing) "" else error.message ?: "The folder was not read",
                          missing, false)
        }
        val device = try { Os.lstat(path).st_dev } catch (_: ErrnoException) { -1L }
        return MeasureWalker(budget, device).folder(path, children, limit, volume)
    }
}

/** The walk was cancelled by ticket; the completion carries the error `cancelled`. */
private class Interrupted : Exception()

/** One child of a measured folder, as one `lstat` saw it: never followed, so a link is a link. */
private class Child(val name: String, val status: StructStat?) {
    /** Hidden as the platform's file manager hides: a leading dot. */
    val hidden = name.startsWith(".")
    val folder = kind == OsConstants.S_IFDIR
    val link = kind == OsConstants.S_IFLNK
    private val kind get() = status?.st_mode?.and(OsConstants.S_IFMT) ?: 0
}

/** A hard-linked file's identity: its volume and its inode, so each file counts once per walk. */
private data class FileKey(val dev: Long, val ino: Long)

/** The walk's shared state: its deadline from the request's start, cancellation by thread
 * interrupt, and the hard links already counted. */
private class Budget(private val deadline: Long) {
    private val started = SystemClock.elapsedRealtime()
    private val seen = HashSet<FileKey>()

    val expired: Boolean get() = SystemClock.elapsedRealtime() - started >= deadline

    /** Stops the walk when the application cancelled its request. */
    fun gate() {
        if (Thread.currentThread().isInterrupted) throw Interrupted()
    }

    /** The bytes a child adds where it is listed: none for a link, none for a hard link already
     * counted, and a folder's own blocks. */
    fun own(child: Child): Long {
        val status = child.status ?: return 0L
        if (child.link) return 0L
        if (child.folder) return status.st_blocks * 512
        return if (seen.add(FileKey(status.st_dev, status.st_ino))) status.st_blocks * 512 else 0L
    }
}

/** Everything below one folder: allocated bytes, the bytes in sight, the items and the folders
 * that could not be read. */
private class Tally(var bytes: Long = 0, var visible: Long = 0, var items: Long = 0, var unreadable: Long = 0) {
    fun add(other: Tally) {
        bytes += other.bytes
        visible += other.visible
        items += other.items
        unreadable += other.unreadable
    }
}

/** One measured child as the slice lists it, with the keys [UI-EFFECTS-MEASURE] documents. */
private class Entry(val name: String, val path: String, own: Long, val hidden: Boolean,
                    val folder: Boolean, val link: Boolean, var guarded: Boolean) {
    var bytes = own
    var visible = if (hidden) 0L else own
    var items = 0L

    fun doc() = Doc.obj(listOf("name" to Doc.str(name), "path" to Doc.str(path), "bytes" to Doc.num(bytes),
        "visible" to Doc.num(visible), "items" to Doc.num(items), "hidden" to Doc.bool(hidden),
        "folder" to Doc.bool(folder), "link" to Doc.bool(link), "guarded" to Doc.bool(guarded)))
}

/** One folder's walk: its children measured to the bottom with an explicit stack, so no tree is
 * deep enough to exhaust the thread's stack. */
private class MeasureWalker(private val budget: Budget, private val device: Long) {
    private var truncated = false

    /** The folder's slice document: its totals over every child and the first `limit` listed. */
    fun folder(path: String, children: List<Child>, limit: Int, volume: String): String {
        truncated = children.size > limit
        val entries = mutableListOf<String>()
        val totals = Tally()
        for (child in children) {
            budget.gate()
            val (entry, unreadable) = measure(path, child)
            totals.bytes += entry.bytes
            totals.visible += entry.visible
            totals.items += 1 + entry.items
            totals.unreadable += unreadable
            if (entries.size < limit) entries.add(entry.doc())
        }
        return opened(path, volume, "", false, truncated, totals, entries)
    }

    /** One direct child: its own bytes plus, for a folder on this volume, everything below it. */
    private fun measure(parent: String, child: Child): Pair<Entry, Long> {
        val path = join(parent, child.name)
        val own = budget.own(child)
        val entry = Entry(child.name, path, own, child.hidden, child.folder, child.link, child.status == null)
        if (!entry.folder || child.status?.st_dev != device) return entry to 0L
        if (budget.expired) {
            truncated = true
            return entry to 0L
        }
        val below = below(path, child.hidden) ?: return guarded(entry)
        entry.bytes = own + below.bytes
        entry.visible = if (child.hidden) 0L else own + below.visible
        entry.items = below.items
        return entry to below.unreadable
    }

    /** A folder below that cannot be read: listed guarded, measuring nothing, and unreadable. */
    private fun guarded(entry: Entry): Pair<Entry, Long> {
        entry.bytes = 0
        entry.visible = 0
        entry.guarded = true
        return entry to 1L
    }

    /** Everything below the folder at `path`, or null when that folder cannot be read. */
    private fun below(path: String, hidden: Boolean): Tally? {
        val first = try { children(path) } catch (_: Exception) { return null }
        val pending = ArrayDeque<Pair<String, Boolean>>()
        val tally = spread(path, hidden, first, pending)
        while (pending.isNotEmpty()) {
            budget.gate()
            val (folder, folderHidden) = pending.removeLast()
            if (budget.expired) {
                truncated = true
                return tally
            }
            val kids = try { children(folder) } catch (_: Exception) { null } ?: run {
                tally.unreadable += 1
                continue
            }
            tally.add(spread(folder, folderHidden, kids, pending))
        }
        return tally
    }

    /** Counts one read folder's children and queues the subfolders the walk may enter. */
    private fun spread(parent: String, hidden: Boolean, children: List<Child>,
                       pending: ArrayDeque<Pair<String, Boolean>>): Tally {
        val tally = Tally()
        for (child in children) {
            val inSight = !hidden && !child.hidden
            val bytes = budget.own(child)
            tally.bytes += bytes
            tally.visible += if (inSight) bytes else 0L
            tally.items += 1
            if (child.folder && child.status?.st_dev == device) pending.addLast(join(parent, child.name) to !inSight)
        }
        return tally
    }
}

/** The children of one folder, each as one `lstat` saw it; a child `lstat` lost to a race is
 * listed guarded. */
private fun children(path: String): List<Child> =
    Files.newDirectoryStream(Paths.get(path)).use { stream ->
        stream.map { entry ->
            val name = entry.fileName.toString()
            Child(name, tryStat(join(path, name)))
        }
    }

private fun tryStat(path: String): StructStat? = try { Os.lstat(path) } catch (_: ErrnoException) { null }

/** `folder` and `name` as one path. */
private fun join(folder: String, name: String): String = File(folder, name).path

/** The volume a folder is on: its capacity and what this user may still write, from statfs
 * ([StatFs] is its wrapper); Linux's statfs carries no volume name, so the name is empty. */
private fun volume(path: String): String =
    listOf(path, File(path).parent ?: "/").firstNotNullOfOrNull { holder ->
        try { volumeOf(StatFs(holder)) } catch (_: Exception) { null }
    } ?: Doc.obj(listOf("name" to Doc.str(""), "total" to Doc.num(0), "available" to Doc.num(0)))

private fun volumeOf(stat: StatFs) =
    Doc.obj(listOf("name" to Doc.str(""), "total" to Doc.num(stat.totalBytes),
                   "available" to Doc.num(stat.availableBytes)))

/** One folder as the slice lists it, with the keys [UI-EFFECTS-MEASURE] documents. */
private fun opened(path: String, volume: String, error: String, missing: Boolean, truncated: Boolean,
                   totals: Tally = Tally(), entries: List<String> = emptyList()) =
    Doc.obj(listOf("path" to Doc.str(path), "error" to Doc.str(error), "missing" to Doc.bool(missing),
        "truncated" to Doc.bool(truncated), "unreadable" to Doc.num(totals.unreadable),
        "bytes" to Doc.num(totals.bytes), "visible" to Doc.num(totals.visible),
        "items" to Doc.num(totals.items), "volume" to volume, "entries" to Doc.arr(entries)))

/** The most entries one measured folder may list, and the longest walk, as the ask's bounds. */
private const val MAX_ENTRIES = 100_000L
private const val MAX_DEADLINE = 600_000L
