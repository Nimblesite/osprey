import Foundation
#if os(iOS)
import Photos
#endif

// Implements [IOS-HOST-IMPORTS]: iOS shows no app another app's files, but it
// lets one measure the photo library once the person allows it. Osprey asks
// with the `photos` command and reads what was found through these functions.
enum PhotoLibrary {
    #if os(iOS)
    nonisolated(unsafe) static var status = "unasked"
    #else
    /// The Mac's photo library is a folder the walk measures.
    nonisolated(unsafe) static var status = "unavailable"
    #endif
    /// Assets and the bytes of them stored on the device, by kind.
    struct Tally {
        var items: Int64 = 0
        var bytes: Int64 = 0
    }

    nonisolated(unsafe) static var items: Int64 = 0
    nonisolated(unsafe) static var bytes: Int64 = 0
    /// Bytes of originals only iCloud holds.
    nonisolated(unsafe) static var cloud: Int64 = 0
    nonisolated(unsafe) static var kinds: [String: Tally] = [:]
    nonisolated(unsafe) private static var names: [String: UnsafePointer<CChar>] = [:]

    /// The status as a string Osprey may keep: one heap copy per status.
    static var statusName: UnsafePointer<CChar> {
        if let name = names[status] { return name }
        let name = UnsafePointer(strdup(status)!)
        names[status] = name
        return name
    }

    /// Asks the first time, then sums every asset off the main thread.
    /// `changed` runs on the main thread after each change of status.
    @MainActor static func measure(_ changed: @escaping @MainActor () -> Void) {
        #if os(iOS)
        PHPhotoLibrary.requestAuthorization(for: .readWrite) { granted in
            Task { @MainActor in
                guard granted == .authorized || granted == .limited else {
                    status = granted == .notDetermined ? "unasked" : "denied"
                    return changed()
                }
                status = "measuring"
                changed()
                let (counted, offDevice) = await Task.detached(priority: .userInitiated) { sum() }.value
                kinds = counted
                items = counted.values.reduce(0) { $0 + $1.items }
                bytes = counted.values.reduce(0) { $0 + $1.bytes }
                cloud = offDevice
                status = granted == .limited ? "limited" : "ready"
                changed()
            }
        }
        #endif
    }

    #if os(iOS)
    /// Every resource of every asset, hidden ones included: the original, any
    /// edited version and a Live Photo's video. A resource counts toward the
    /// device only when it is stored there; with Optimise iPhone Storage on,
    /// iCloud keeps the original and the device a smaller copy Photos manages.
    private static func sum() -> ([String: Tally], Int64) {
        let options = PHFetchOptions()
        options.includeHiddenAssets = true
        let assets = PHAsset.fetchAssets(with: options)
        var counted: [String: Tally] = [:], offDevice: Int64 = 0
        assets.enumerateObjects { asset, _, _ in
            let name = kind(asset)
            counted[name, default: Tally()].items += 1
            for resource in PHAssetResource.assetResources(for: asset) {
                let size = number(resource, "fileSize")
                if number(resource, "locallyAvailable", or: 1) != 0 {
                    counted[name, default: Tally()].bytes += size
                } else {
                    offDevice += size
                }
            }
        }
        return (counted, offDevice)
    }

    private static func kind(_ asset: PHAsset) -> String {
        if asset.mediaType == .video { return "videos" }
        if asset.mediaSubtypes.contains(.photoScreenshot) { return "screenshots" }
        if asset.mediaSubtypes.contains(.photoLive) { return "live" }
        return "photos"
    }

    /// A resource property PhotoKit reports but does not declare; `fallback`
    /// when this iOS does not have it.
    private static func number(_ resource: PHAssetResource, _ key: String, or fallback: Int64 = 0) -> Int64 {
        let getter = key == "locallyAvailable" ? "isLocallyAvailable" : key
        guard resource.responds(to: NSSelectorFromString(getter)) || resource.responds(to: NSSelectorFromString(key)) else {
            return fallback
        }
        return (resource.value(forKey: key) as? NSNumber)?.int64Value ?? fallback
    }
    #endif
}

@_cdecl("storage_photos_status")
func storagePhotosStatus() -> UnsafePointer<CChar>? { PhotoLibrary.statusName }

@_cdecl("storage_photos_items")
func storagePhotosItems() -> Int64 { PhotoLibrary.items }

@_cdecl("storage_photos_bytes")
func storagePhotosBytes() -> Int64 { PhotoLibrary.bytes }

@_cdecl("storage_photos_cloud")
func storagePhotosCloud() -> Int64 { PhotoLibrary.cloud }

@_cdecl("storage_photos_kind_items")
func storagePhotosKindItems(_ kind: UnsafePointer<CChar>?) -> Int64 {
    kind.flatMap { PhotoLibrary.kinds[String(cString: $0)]?.items } ?? 0
}

@_cdecl("storage_photos_kind_bytes")
func storagePhotosKindBytes(_ kind: UnsafePointer<CChar>?) -> Int64 {
    kind.flatMap { PhotoLibrary.kinds[String(cString: $0)]?.bytes } ?? 0
}
