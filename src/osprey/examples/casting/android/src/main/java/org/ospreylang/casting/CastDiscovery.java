package org.ospreylang.casting;

import android.content.Context;
import android.net.nsd.NsdManager;
import android.net.nsd.NsdServiceInfo;
import android.os.Handler;
import android.os.Looper;
import java.nio.charset.StandardCharsets;
import java.util.TreeMap;
import java.util.ArrayList;
import java.util.function.Consumer;

// [CAST-MOBILE-DELIVERY] Native NSD replaces desktop dns-sd subprocesses.
public final class CastDiscovery implements NsdManager.DiscoveryListener {
    private final NsdManager manager;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final TreeMap<String, String> records = new TreeMap<>();
    private Consumer<String> success;
    private Consumer<Exception> failure;
    private boolean active;
    private int generation;
    private final ArrayList<NsdManager.ServiceInfoCallback> callbacks = new ArrayList<>();
    private final Runnable deadline = () -> finish(null);

    public CastDiscovery(Context context) {
        manager = (NsdManager) context.getSystemService(Context.NSD_SERVICE);
    }

    public void start(Consumer<String> success, Consumer<Exception> failure) {
        cancel();
        this.success = success;
        this.failure = failure;
        records.clear();
        active = true;
        try {
            manager.discoverServices("_googlecast._tcp.", NsdManager.PROTOCOL_DNS_SD, this);
            main.postDelayed(deadline, 3000);
        } catch (RuntimeException error) { finish(error); }
    }

    public void cancel() {
        if (active) finish(new java.util.concurrent.CancellationException("Discovery cancelled"));
    }

    private void finish(Exception error) {
        if (!active) return;
        active = false;
        generation++;
        main.removeCallbacks(deadline);
        callbacks.forEach(callback -> {
            try { manager.unregisterServiceInfoCallback(callback); }
            catch (IllegalArgumentException ignored) { /* NSD already unregistered this callback. */ }
        });
        callbacks.clear();
        try { manager.stopServiceDiscovery(this); } catch (IllegalArgumentException ignored) { /* Already stopped by NSD. */ }
        Consumer<String> done = success;
        Consumer<Exception> failed = failure;
        success = null;
        failure = null;
        if (error == null) done.accept(String.join("\n", records.values()));
        else failed.accept(error);
    }

    @Override public void onServiceFound(NsdServiceInfo service) {
        main.post(() -> watch(service));
    }

    private void watch(NsdServiceInfo service) {
        if (!active) return;
        final int epoch = generation;
        NsdManager.ServiceInfoCallback callback = new NsdManager.ServiceInfoCallback() {
            @Override public void onServiceInfoCallbackRegistrationFailed(int code) {
                if (active && generation == epoch) finish(new IllegalStateException("Receiver resolution failed: " + code));
            }
            @Override public void onServiceUpdated(NsdServiceInfo info) { record(info, epoch); }
            @Override public void onServiceLost() {
                if (active && generation == epoch) records.remove(service.getServiceName());
            }
            @Override public void onServiceInfoCallbackUnregistered() {}
        };
        callbacks.add(callback);
        try { manager.registerServiceInfoCallback(service, command -> main.post(command), callback); }
        catch (RuntimeException error) { callbacks.remove(callback); finish(error); }
    }

    private void record(NsdServiceInfo info, int epoch) {
        if (!active || generation != epoch || info.getHostAddresses().isEmpty() || info.getPort() <= 0) return;
        byte[] friendly = info.getAttributes().get("fn");
        String name = friendly == null ? info.getServiceName() : new String(friendly, StandardCharsets.UTF_8);
        String key = "cast-" + Integer.toUnsignedString(info.getServiceName().hashCode());
        records.put(info.getServiceName(), transcript(key, name, info.getHostAddresses().get(0).getHostAddress(), info.getPort()));
    }

    static String transcript(String key, String name, String host, int port) {
        String safe = name.replace("\\", "\\\\").replace("\"", "").replace('\n', ' ').replace('\r', ' ');
        return key + " SRV 0 0 " + port + " " + host + "\n" + key + " TXT \"fn=" + safe + "\"";
    }

    @Override public void onServiceLost(NsdServiceInfo service) { main.post(() -> records.remove(service.getServiceName())); }
    @Override public void onDiscoveryStarted(String type) {}
    @Override public void onDiscoveryStopped(String type) {}
    @Override public void onStartDiscoveryFailed(String type, int code) {
        main.post(() -> finish(new IllegalStateException("Local network discovery failed: " + code)));
    }
    @Override public void onStopDiscoveryFailed(String type, int code) {}
}
