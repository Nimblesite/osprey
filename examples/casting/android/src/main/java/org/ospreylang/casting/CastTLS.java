package org.ospreylang.casting;

import java.io.IOException;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.security.GeneralSecurityException;
import java.security.cert.X509Certificate;
import java.security.cert.CertificateException;
import java.util.Arrays;
import java.util.HashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLSocket;
import javax.net.ssl.TrustManager;
import javax.net.ssl.X509TrustManager;

// [CAST-MOBILE-DELIVERY] Deliberately unverified TLS for discovered Cast
// receivers with self-signed certificates. Never use this transport for HTTPS.
public final class CastTLS {
    private static final int LIMIT = 1 << 24;
    private static final HashMap<Long, Session> sessions = new HashMap<>();
    private static final ExecutorService writes = Executors.newSingleThreadExecutor(task -> {
        Thread thread = new Thread(task, "casting-tls-write");
        thread.setDaemon(true);
        return thread;
    });
    private CastTLS() {}

    private static final class Session {
        final SSLSocket socket;
        byte[] pending = new byte[0];
        int received;
        int status;
        Session(SSLSocket socket) { this.socket = socket; }
    }

    public static synchronized long connect(String host, long port, long timeout) {
        if (host == null || host.isEmpty() || port < 1 || port > 65535 || timeout < 1 || timeout > 60000) return -1;
        long handle = 1;
        while (handle <= 64 && sessions.containsKey(handle)) handle++;
        if (handle > 64) return -5;
        SSLSocket socket = null;
        long failure = -6;
        try {
            SSLContext context = context();
            failure = -3; socket = (SSLSocket) context.getSocketFactory().createSocket();
            failure = -4; socket.connect(new InetSocketAddress(host, (int) port), (int) timeout);
            failure = -7;
            socket.setSoTimeout((int) timeout);
            socket.startHandshake();
            sessions.put(handle, new Session(socket));
            return handle;
        } catch (GeneralSecurityException | IOException error) {
            if (socket != null) try { socket.close(); } catch (IOException cleanup) { error.addSuppressed(cleanup); }
            return error instanceof UnknownHostException ? -2 : failure;
        }
    }

    // Implements [FFI-TLS-UNVERIFIED], matching the existing Cast-only Wire
    // contract: device identity is unauthenticated, unlike HTTPS. The narrow
    // exception permits this explicit local-device policy, not general trust bypass.
    @android.annotation.SuppressLint("CustomX509TrustManager")
    private static SSLContext context() throws GeneralSecurityException {
        SSLContext context = SSLContext.getInstance("TLS");
        context.init(null, new TrustManager[]{new X509TrustManager() {
            @Override public void checkClientTrusted(X509Certificate[] chain, String auth) throws CertificateException {
                throw new CertificateException("This Cast sender never authenticates TLS clients");
            }
            @Override public void checkServerTrusted(X509Certificate[] chain, String auth) throws CertificateException {
                if (chain == null || chain.length == 0) throw new CertificateException("Receiver sent no certificate");
                for (X509Certificate certificate : chain) certificate.checkValidity();
                try {
                    for (int i = 0; i + 1 < chain.length; i++) chain[i].verify(chain[i + 1].getPublicKey());
                    if (chain.length == 1) chain[0].verify(chain[0].getPublicKey());
                } catch (GeneralSecurityException error) { throw new CertificateException("Invalid Cast certificate signature", error); }
            }
            @Override public X509Certificate[] getAcceptedIssuers() { return new X509Certificate[0]; }
        }}, null);
        return context;
    }

    public static synchronized long write(long handle, String hex) {
        Session session = sessions.get(handle);
        if (session == null) return -1;
        byte[] bytes = decode(hex);
        if (bytes == null) { session.status = 3; return -1; }
        Future<?> task = writes.submit(() -> {
            session.socket.getOutputStream().write(bytes);
            session.socket.getOutputStream().flush();
            return null;
        });
        try { task.get(5, TimeUnit.SECONDS); session.status = 0; return 0; }
        catch (Exception error) {
            task.cancel(true);
            try { session.socket.close(); } catch (IOException cleanup) { error.addSuppressed(cleanup); }
            if (error instanceof InterruptedException) Thread.currentThread().interrupt();
            session.status = 3;
            return -1;
        }
    }

    private static byte[] decode(String hex) {
        if (hex == null || hex.length() % 2 != 0 || hex.length() > LIMIT * 2) return null;
        byte[] bytes = new byte[hex.length() / 2];
        for (int index = 0; index < bytes.length; index++) {
            int hi = Character.digit(hex.charAt(index * 2), 16);
            int lo = Character.digit(hex.charAt(index * 2 + 1), 16);
            if (hi < 0 || lo < 0 || hex.charAt(index * 2) > 127 || hex.charAt(index * 2 + 1) > 127) return null;
            bytes[index] = (byte) ((hi << 4) | lo);
        }
        return bytes;
    }

    public static synchronized String read(long handle, long count, long timeout) {
        Session session = sessions.get(handle);
        if (session == null) return "";
        if (count < 1 || count > LIMIT || timeout < 0 || timeout > 60000) { session.status = 3; return ""; }
        if (session.status >= 2 || session.received > count) return "";
        session.pending = Arrays.copyOf(session.pending, (int) count);
        long deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeout);
        session.status = 0;
        try {
            while (session.received < count) {
                long remaining = deadline - System.nanoTime();
                if (remaining <= 0) { session.status = 1; return ""; }
                session.socket.setSoTimeout((int) Math.max(1, TimeUnit.NANOSECONDS.toMillis(remaining)));
                int got = session.socket.getInputStream().read(session.pending, session.received, (int) count - session.received);
                if (got < 0) { session.status = 2; return ""; }
                session.received += got;
            }
            String value = encode(session.pending);
            session.received = 0;
            return value;
        } catch (SocketTimeoutException error) { session.status = 1; return ""; }
        catch (IOException error) { session.status = 3; return ""; }
    }

    private static String encode(byte[] bytes) {
        char[] hex = new char[bytes.length * 2];
        String digits = "0123456789abcdef";
        for (int index = 0; index < bytes.length; index++) {
            hex[index * 2] = digits.charAt((bytes[index] & 255) >>> 4);
            hex[index * 2 + 1] = digits.charAt(bytes[index] & 15);
        }
        return new String(hex);
    }

    public static synchronized long status(long handle) {
        Session session = sessions.get(handle);
        return session == null ? -1 : session.status;
    }

    public static synchronized long close(long handle) {
        Session session = sessions.remove(handle);
        if (session == null) return -1;
        try { session.socket.close(); return 0; } catch (IOException error) { return -1; }
    }

    public static String route(String host, long port) {
        if (host == null || port < 1 || port > 65535) return "";
        try (DatagramSocket socket = new DatagramSocket()) {
            socket.connect(InetAddress.getByName(host), (int) port);
            return socket.getLocalAddress().getHostAddress();
        } catch (IOException error) { return ""; }
    }
}
