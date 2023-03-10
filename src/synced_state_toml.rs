use std::{borrow::Borrow, path::Path, sync::Arc};

use serde::{Serialize, Deserialize};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, MutexGuard};
use anyhow::Result;
use crate::{synced_state::Synced, saveable_state::SaveableToml};

pub type SyncedToml<T> = Synced<SaveableToml<T>>;

impl<T> Synced<SaveableToml<T>>
where T: Default + Serialize + for<'a> Deserialize<'a> + Clone
{
    pub async fn init(
        key: impl Into<String>,
        relative_path: impl AsRef<Path>,
        handle: impl Borrow<AppHandle>
    ) -> Self {

        let handle = handle.borrow();
        let key: String = key.into();

        let mut path = handle.path_resolver()
            .app_config_dir()
            .expect("Failed to resolve app config directory");

        path.push(relative_path);

        let state = SaveableToml::<T>::load_path(&path)
            .await
            .unwrap_or_else(|error| {
                eprintln!("Failed to initialize '{key}' state: {error}");
                SaveableToml::<T>::new(&path)
            });

        Self {
            key,
            state: Arc::new(Mutex::new(
                state
            )),
            handle: handle.clone(),
        }
    }

    pub fn init_sync(
        key: impl Into<String>,
        relative_path: impl AsRef<Path>,
        handle: impl Borrow<AppHandle>
    ) -> Self {
        tokio::task::block_in_place(|| {
            tauri::async_runtime::block_on(Self::init(key, relative_path, handle))
        })
    }

    fn emit_update(&self, payload: T) {
        let key = &self.key;
        let handle = &self.handle;
        let event = format!("synced-state://{key}-update");

        handle
            .emit_all(event.as_str(), payload)
            .ok();
    }

    pub async fn mutate(
        &self,
        function: impl FnOnce(&mut T)
    ) {
        let mut lock = self.state.lock().await;
        let state = &mut lock.state;

        function(state);

        self.emit_update(state.to_owned());
        lock.save().await.ok();
    }

    pub async fn save(&self) -> Result<()> {
        self.state
            .lock()
            .await
            .save()
            .await
    }

    pub fn save_sync(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            tauri::async_runtime::block_on(self.save())
        })
    }

    pub async fn get(&self) -> T {
        let lock = self.state.lock().await;
        lock.state.clone()
    }

    pub async fn set(&self, new_value: T) {
        self.mutate(|value| {
            *value = new_value.clone();
        }).await;
    }

    pub async fn lock(&self) -> MutexGuard<SaveableToml<T>> {
        self.state.lock().await
    }

    pub async fn reset(&self) {
        self.set(T::default()).await;
    }
}