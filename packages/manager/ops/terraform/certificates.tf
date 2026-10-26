# DNS authorization allows issuance while the origin records stay proxied.
# No temporary public-origin or TLS-validation bypass is necessary.
resource "google_certificate_manager_dns_authorization" "origin" {
  for_each   = local.origin_hostnames
  name       = "${local.name}-${each.key}"
  domain     = each.value
  depends_on = [google_project_service.required]
}

resource "cloudflare_dns_record" "certificate_authorization" {
  for_each = local.origin_hostnames
  zone_id  = var.cloudflare_zone_id
  name     = google_certificate_manager_dns_authorization.origin[each.key].dns_resource_record[0].name
  type     = google_certificate_manager_dns_authorization.origin[each.key].dns_resource_record[0].type
  content  = google_certificate_manager_dns_authorization.origin[each.key].dns_resource_record[0].data
  proxied  = false
  ttl      = 300
}

resource "google_certificate_manager_certificate" "origin" {
  name = local.name
  managed {
    domains            = values(local.origin_hostnames)
    dns_authorizations = [for authorization in google_certificate_manager_dns_authorization.origin : authorization.id]
  }
  depends_on = [cloudflare_dns_record.certificate_authorization]
}

resource "google_certificate_manager_certificate_map" "origin" {
  name       = local.name
  depends_on = [google_project_service.required]
}

resource "google_certificate_manager_certificate_map_entry" "origin" {
  for_each     = local.origin_hostnames
  name         = "${local.name}-${each.key}"
  map          = google_certificate_manager_certificate_map.origin.name
  hostname     = each.value
  certificates = [google_certificate_manager_certificate.origin.id]
}
