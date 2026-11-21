// [PRISM-ROUTING-VST-WORKER] Own one vendor-provided Cocoa editor on its UI thread.
#import <Cocoa/Cocoa.h>
#include "processor.h"
#include "pluginterfaces/gui/iplugview.h"
#include <charconv>
#include <poll.h>
#include <unistd.h>
#include <map>
#include <sstream>
#include <iomanip>

namespace {
using namespace osprey::vst;
constexpr size_t maxLine = 131072;
class Frame final : public U::Implements<U::Directly<IPlugFrame>> {
public:
    NSWindow* window = nil;
    tresult PLUGIN_API resizeView(IPlugView* view, ViewRect* size) override {
        if (!view || !size || !valid(*size)) return kInvalidArgument;
        [window setContentSize:NSMakeSize(size->getWidth(), size->getHeight())];
        return view->onSize(size);
    }
    static bool valid(ViewRect rect) {
        const int64 width = int64(rect.right) - rect.left, height = int64(rect.bottom) - rect.top;
        return width > 0 && width <= 8192 && height > 0 && height <= 8192;
    }
};

class EditorHandler final : public U::Implements<U::Directly<IComponentHandler>> {
public:
    tresult PLUGIN_API beginEdit(ParamID) override { return kResultOk; }
    tresult PLUGIN_API performEdit(ParamID id, ParamValue value) override {
        if (!std::isfinite(value) || value < 0 || value > 1 || (!edits.contains(id) && edits.size() >= 1024))
            return kInvalidArgument;
        edits[id] = value;
        return kResultOk;
    }
    tresult PLUGIN_API endEdit(ParamID) override { return kResultOk; }
    tresult PLUGIN_API restartComponent(int32) override { return kResultOk; }
    std::map<ParamID,double> edits;
};

class VendorWindow {
public:
    Plugin plugin;
    IPtr<EditorHandler> handler = owned(new EditorHandler);
    IPtr<IEditController> controller;
    IPtr<IPlugView> view;
    IPtr<Frame> frame = owned(new Frame);
    bool attached = false;
    std::map<ParamID, double> authored, observed;
    ~VendorWindow() {
        if (attached) view->removed();
        if (view) view->setFrame(nullptr);
        [frame->window orderOut:nil];
    }
    bool connect(const std::string& path, const std::string& id) {
        if (!plugin.open(path, id)) return false;
        controller = plugin.provider->getControllerPtr();
        plugin.error = "This VST3 plug-in does not provide a Cocoa editor.";
        if (!controller || controller->setComponentHandler(handler) != kResultOk) return false;
        view = owned(controller->createView(ViewType::kEditor));
        if (!view || view->isPlatformTypeSupported(kPlatformTypeNSView) != kResultOk) return false;
        ViewRect size{};
        if (view->getSize(&size) != kResultOk || !Frame::valid(size)) return false;
        NSString* title = @"VST3 Editor";
        for (const auto& info : plugin.module->getFactory().classInfos())
            if (info.ID().toString() == id) title = [NSString stringWithUTF8String:info.name().c_str()];
        frame->window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0,0,size.getWidth(),size.getHeight())
            styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskMiniaturizable
            backing:NSBackingStoreBuffered defer:NO];
        frame->window.releasedWhenClosed = NO;
        frame->window.title = title;
        NSView* container = [[NSView alloc] initWithFrame:frame->window.contentView.bounds];
        container.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        [frame->window.contentView addSubview:container];
        view->setFrame(frame);
        attached = view->attached((__bridge void*)container, kPlatformTypeNSView) == kResultOk;
        if (attached) {
            [frame->window center];
            [frame->window makeKeyAndOrderFront:nil];
            [NSApp activateIgnoringOtherApps:YES];
        }
        return attached;
    }

    // Only host-authored changes are sent back to the controller. Polling must preserve an
    // in-progress gesture in the vendor window while the host still has its preceding value.
    bool apply(const std::string& token) {
        const auto split = token.find('=');
        if (split == std::string::npos) return false;
        ParamID id{};
        const auto parsed = std::from_chars(token.data(), token.data()+split, id);
        if (parsed.ec != std::errc{} || parsed.ptr != token.data()+split) return false;
        char* end = nullptr;
        const char* number = token.c_str()+split+1;
        const double value = std::strtod(number, &end);
        if (end == number || *end || !std::isfinite(value) || value < 0 || value > 1) return false;
        if (!authored.contains(id) || authored[id] != value) {
            if (controller->getParamNormalized(id) != value && controller->setParamNormalized(id,value) != kResultOk)
                return false;
            handler->edits.erase(id);
            authored[id] = value;
        }
        return true;
    }
    bool reply(const std::string& line) {
        std::istringstream request(line);
        std::string command;
        request >> command;
        if (command != "watch" && command != "focus" && command != "close") return false;
        if (command == "focus") { [frame->window makeKeyAndOrderFront:nil]; [NSApp activateIgnoringOtherApps:YES]; }
        if (command == "close") [frame->window close];
        for (std::string token; request >> token;) if (!apply(token) || authored.size() > 1024) return false;
        std::ostringstream out;
        out << "{\"width\":" << frame->window.contentView.bounds.size.width
            << ",\"height\":" << frame->window.contentView.bounds.size.height
            << ",\"window\":" << frame->window.windowNumber << ",\"closed\":" << (frame->window.visible ? "false" : "true") << ",\"values\":[";
        bool first = true;
        for (const auto& [id, _] : authored) {
            const auto native = handler->edits.find(id);
            const double value = native == handler->edits.end() ? controller->getParamNormalized(id) : native->second;
            if (!std::isfinite(value) || value < 0 || value > 1) return false;
            if (!observed.contains(id) || observed[id] != value) {
                if (!first) out << ',';
                out << "{\"id\":" << id << ",\"value\":" << std::setprecision(17) << value << '}';
                first = false;
                observed[id] = value;
            }
        }
        handler->edits.clear();
        std::cout << out.str() << "]}" << std::endl;
        return bool(std::cout);
    }
};
// Service the protocol from Cocoa's run loop, including nested loops entered by a vendor view.
int service(VendorWindow& editor, std::string& pending) {
    pollfd input{STDIN_FILENO, POLLIN, 0};
    if (poll(&input, 1, 0) < 0) return failed("VST3 editor input failed.");
    if (!input.revents) return -1;
    char bytes[4096];
    const auto count = read(STDIN_FILENO, bytes, sizeof bytes);
    if (count == 0) return 0;
    if (count < 0 || pending.size() + count > maxLine) return failed("Invalid VST3 editor request.");
    pending.append(bytes, count);
    size_t end;
    while ((end = pending.find('\n')) != std::string::npos) {
        if (!editor.reply(pending.substr(0, end))) return failed("Invalid VST3 editor controls.");
        pending.erase(0, end + 1);
        if (!editor.frame->window.visible) return 0;
    }
    return -1;
}
}
int editPlugin(const std::string& path, const std::string& identifier) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [NSApp finishLaunching];
        VendorWindow editor;
        if (!editor.connect(path, identifier)) return failed(editor.plugin.error);
        std::string pending;
        __block int status = -1;
        VendorWindow* active = &editor;
        std::string* buffer = &pending;
        NSTimer* timer = [NSTimer timerWithTimeInterval:0.04 repeats:YES block:^(NSTimer*) {
            status = service(*active, *buffer);
            if (status >= 0) [NSApp stop:nil];
        }];
        [NSRunLoop.mainRunLoop addTimer:timer forMode:NSRunLoopCommonModes];
        [NSApp run];
        [timer invalidate];
        return status < 0 ? failed("VST3 editor event loop stopped.") : status;
    }
}
