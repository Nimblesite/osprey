variable "environment" {
  type = string
  validation {
    condition     = contains(["staging", "production"], var.environment)
    error_message = "Use an isolated staging or production environment."
  }
}

variable "project_id" {
  type = string
}

variable "region" {
  type    = string
  default = "australia-southeast1"
}

variable "cloudflare_account_id" {
  type = string
}

variable "cloudflare_zone_id" {
  type = string
}

variable "api_hostname" {
  type = string
  validation {
    condition     = can(regex("^[a-z0-9-]+(\\.[a-z0-9-]+)+$", var.api_hostname))
    error_message = "Supply the approved API DNS hostname without a URL path."
  }
}

variable "website_hostname" {
  type = string
}

variable "content_hostname" {
  type = string
}

variable "image" {
  type = string
  validation {
    condition     = can(regex("^[a-z0-9.-]+/[^@]+@sha256:[a-f0-9]{64}$", var.image))
    error_message = "Deploy an audited immutable Artifact Registry image digest, never a mutable tag."
  }
}

variable "max_instances" {
  type    = number
  default = 5
  validation {
    condition     = var.max_instances >= 1 && var.max_instances <= 50 && floor(var.max_instances) == var.max_instances
    error_message = "Use an explicit integer instance cap between one and fifty."
  }
}

variable "runtime_config_secret_version" {
  type = string
  validation {
    condition     = can(regex("^[1-9][0-9]*$", var.runtime_config_secret_version))
    error_message = "Pin a provisioned runtime configuration secret's numeric version."
  }
}

variable "github_repository" {
  type = string
  validation {
    condition     = can(regex("^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$", var.github_repository))
    error_message = "Use the exact GitHub owner/repository identity."
  }
}

variable "github_repository_id" {
  type = string
  validation {
    condition     = can(regex("^[1-9][0-9]*$", var.github_repository_id))
    error_message = "Pin the stable numeric repository ID, not just its reusable name."
  }
}

variable "github_owner_id" {
  type = string
  validation {
    condition     = can(regex("^[1-9][0-9]*$", var.github_owner_id))
    error_message = "Pin the stable numeric repository owner ID."
  }
}

locals {
  name = "osprey-registry-${var.environment}"
  required_services = toset([
    "artifactregistry.googleapis.com", "run.googleapis.com", "compute.googleapis.com",
    "secretmanager.googleapis.com", "iam.googleapis.com", "iamcredentials.googleapis.com",
    "sts.googleapis.com", "logging.googleapis.com", "monitoring.googleapis.com",
    "certificatemanager.googleapis.com"
  ])
  origin_hostnames = { api = var.api_hostname, content = var.content_hostname }
}
