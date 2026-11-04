# Implements [PACKAGE-HOSTED-TOPOLOGY], [PACKAGE-HOSTED-EDGE].
resource "google_project_service" "required" {
  for_each           = local.required_services
  service            = each.value
  disable_on_destroy = false
}

resource "google_artifact_registry_repository" "registry" {
  location      = var.region
  repository_id = local.name
  format        = "DOCKER"
  depends_on    = [google_project_service.required]
}

resource "google_service_account" "runtime" {
  account_id   = "registry-${var.environment}-runtime"
  display_name = "Registry runtime; no deployment or migration authority"
}

resource "google_service_account" "deploy" {
  account_id   = "registry-${var.environment}-deploy"
  display_name = "Registry exact-SHA deployment identity"
}

resource "google_service_account" "incident" {
  account_id   = "registry-${var.environment}-incident"
  display_name = "Registry independent incident job; grant DB deny-only credentials separately"
}

resource "google_secret_manager_secret" "runtime_config" {
  secret_id = "${local.name}-runtime-config"
  replication {
    auto {}
  }
  lifecycle {
    prevent_destroy = true
  }
  depends_on = [google_project_service.required]
}

resource "google_secret_manager_secret_iam_member" "runtime_config" {
  secret_id = google_secret_manager_secret.runtime_config.id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.runtime.email}"
}

resource "google_cloud_run_v2_service" "api" {
  name                 = local.name
  location             = var.region
  ingress              = "INGRESS_TRAFFIC_INTERNAL_LOAD_BALANCER"
  default_uri_disabled = true
  deletion_protection  = true
  template {
    service_account                  = google_service_account.runtime.email
    timeout                          = "30s"
    max_instance_request_concurrency = 16
    scaling {
      min_instance_count = 0
      max_instance_count = var.max_instances
    }
    containers {
      image = var.image
      ports {
        container_port = 8080
      }
      resources {
        limits = { cpu = "1", memory = "512Mi" }
      }
      volume_mounts {
        name       = "runtime-config"
        mount_path = "/run/registry"
      }
      startup_probe {
        http_get {
          path = "/v1/ready"
          port = 8080
        }
        period_seconds    = 5
        failure_threshold = 12
        timeout_seconds   = 2
      }
    }
    volumes {
      name = "runtime-config"
      secret {
        secret = google_secret_manager_secret.runtime_config.secret_id
        items {
          version = var.runtime_config_secret_version
          path    = "runtime.json"
          mode    = 292
        }
      }
    }
  }
  depends_on = [google_project_service.required, google_secret_manager_secret_iam_member.runtime_config]
}

# The public load balancer invokes the service. Its network ingress is restricted
# above; IAM alone must not be mistaken for Cloudflare origin authentication.
resource "google_cloud_run_v2_service_iam_member" "load_balancer" {
  project  = var.project_id
  location = var.region
  name     = google_cloud_run_v2_service.api.name
  role     = "roles/run.invoker"
  member   = "allUsers"
}

resource "google_compute_region_network_endpoint_group" "api" {
  name                  = local.name
  region                = var.region
  network_endpoint_type = "SERVERLESS"
  cloud_run {
    service = google_cloud_run_v2_service.api.name
  }
}

data "cloudflare_ip_ranges" "edge" {}

resource "google_compute_security_policy" "edge" {
  name = local.name
  dynamic "rule" {
    for_each = chunklist(data.cloudflare_ip_ranges.edge.ipv4_cidrs, 10)
    content {
      action   = "allow"
      priority = 1000 + rule.key
      match {
        versioned_expr = "SRC_IPS_V1"
        config {
          src_ip_ranges = rule.value
        }
      }
    }
  }
  rule {
    action   = "deny(403)"
    priority = 2147483647
    match {
      versioned_expr = "SRC_IPS_V1"
      config {
        src_ip_ranges = ["*"]
      }
    }
  }
}

resource "google_compute_backend_service" "api" {
  name                  = local.name
  protocol              = "HTTP"
  load_balancing_scheme = "EXTERNAL_MANAGED"
  security_policy       = google_compute_security_policy.edge.id
  enable_cdn            = false
  backend {
    group = google_compute_region_network_endpoint_group.api.id
  }
  log_config {
    enable      = true
    sample_rate = 1
  }
}

resource "google_compute_url_map" "api" {
  name            = local.name
  default_service = google_compute_backend_service.api.id
}

resource "google_compute_ssl_policy" "api" {
  name            = local.name
  min_tls_version = "TLS_1_2"
  profile         = "MODERN"
}

resource "google_compute_target_https_proxy" "api" {
  name            = local.name
  url_map         = google_compute_url_map.api.id
  certificate_map = "//certificatemanager.googleapis.com/${google_certificate_manager_certificate_map.origin.id}"
  ssl_policy      = google_compute_ssl_policy.api.id
}

resource "google_compute_global_address" "api" {
  name = local.name
}

resource "google_compute_global_forwarding_rule" "api" {
  name                  = local.name
  ip_address            = google_compute_global_address.api.address
  target                = google_compute_target_https_proxy.api.id
  port_range            = "443"
  load_balancing_scheme = "EXTERNAL_MANAGED"
}

resource "cloudflare_dns_record" "api" {
  for_each = local.origin_hostnames
  zone_id  = var.cloudflare_zone_id
  name     = each.value
  type     = "A"
  content  = google_compute_global_address.api.address
  proxied  = true
  ttl      = 1
}

resource "cloudflare_pages_project" "website" {
  account_id        = var.cloudflare_account_id
  name              = local.name
  production_branch = "main"
}

resource "cloudflare_pages_domain" "website" {
  account_id   = var.cloudflare_account_id
  project_name = cloudflare_pages_project.website.name
  name         = var.website_hostname
  depends_on   = [cloudflare_dns_record.website]
}

resource "cloudflare_dns_record" "website" {
  zone_id = var.cloudflare_zone_id
  name    = var.website_hostname
  type    = "CNAME"
  content = cloudflare_pages_project.website.subdomain
  proxied = true
  ttl     = 1
}
