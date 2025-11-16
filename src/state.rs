use parking_lot::{Mutex, RwLock};
use std::collections::VecDeque;
use tracing::info;
use uuid::Uuid;

use crate::http_client::SharedHttpClient;
use crate::settings::{RequestContext, Settings, SettingsLayer, matches_request};

pub struct AppState {
    env_layer: SettingsLayer,
    admin_overrides: RwLock<SettingsLayer>,
    one_off: Mutex<VecDeque<OneOffRule>>,
    client: SharedHttpClient,
    body_trailer: String,
}

struct OneOffRule {
    id: Uuid,
    settings: Settings,
}

impl AppState {
    pub fn new(env_layer: SettingsLayer, body_trailer: String, client: SharedHttpClient) -> Self {
        Self {
            env_layer,
            admin_overrides: RwLock::new(SettingsLayer::default()),
            one_off: Mutex::new(VecDeque::new()),
            client,
            body_trailer,
        }
    }

    pub fn log_env_overrides(&self) {
        for (key, value) in self.env_layer.entries() {
            info!("env setting {key} {value}");
        }
    }

    pub fn body_trailer(&self) -> &str {
        &self.body_trailer
    }

    pub fn client(&self) -> SharedHttpClient {
        self.client.clone()
    }

    pub fn merge_admin(&self, layer: SettingsLayer) -> Settings {
        let mut guard = self.admin_overrides.write();
        guard.merge(&layer);
        self.snapshot_locked(&guard)
    }

    pub fn reset_admin(&self, layer: SettingsLayer) -> Settings {
        let mut guard = self.admin_overrides.write();
        *guard = layer;
        self.snapshot_locked(&guard)
    }

    pub fn admin_snapshot(&self) -> Settings {
        let guard = self.admin_overrides.read();
        self.snapshot_locked(&guard)
    }

    pub fn effective_settings(&self, overrides: &SettingsLayer) -> Settings {
        let mut snapshot = self.admin_snapshot();
        snapshot.apply_layer(overrides);
        snapshot
    }

    pub fn add_one_off(&self, mut settings: Settings) -> Uuid {
        let id = Uuid::new_v4();
        settings.destination_url = None;
        self.one_off.lock().push_back(OneOffRule { id, settings });
        info!("Added one-off rule {id}");
        id
    }

    pub fn apply_one_off(&self, ctx: &RequestContext, current: Settings) -> Settings {
        let mut guard = self.one_off.lock();
        if guard.is_empty() {
            return current;
        }
        let destination = current.destination_url.clone();
        let idx = guard.iter().position(|rule| {
            let mut candidate = rule.settings.clone();
            candidate.destination_url = destination.clone();
            matches_request(ctx, &candidate)
        });

        if let Some(idx) = idx {
            let mut rule = guard.remove(idx).expect("one-off rule");
            rule.settings.destination_url = destination;
            info!("Consuming one-off rule {}", rule.id);
            rule.settings
        } else {
            current
        }
    }

    fn snapshot_locked(&self, admin: &SettingsLayer) -> Settings {
        let mut settings = Settings::default();
        settings.apply_layer(&self.env_layer);
        settings.apply_layer(admin);
        settings
    }
}
