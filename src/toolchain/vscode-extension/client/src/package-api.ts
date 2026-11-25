// Implements [PACKAGE-API]: validate the configured registry origin before opening the package workspace.
const loopbackHosts = ["127.0.0.1", "localhost", "[::1]"];

export function registryUrl(origin: string): URL {
  const url = new URL(origin);
  const loopback = loopbackHosts.includes(url.hostname);
  if ((url.protocol !== "https:" && !(url.protocol === "http:" && loopback)) ||
      url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
    throw new Error("Package registry must be an HTTPS origin (HTTP is allowed only on loopback).");
  }
  url.pathname = "/v1/packages";
  return url;
}
