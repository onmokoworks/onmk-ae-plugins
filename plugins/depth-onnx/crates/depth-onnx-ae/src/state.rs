use depth_onnx_core::{
    default_scan_roots, load_manifest, scan_model_roots, BundledModel, ModelManifest,
};

#[derive(Clone, Copy, Debug)]
pub enum Resolution {
    R266,
    R392,
    R518,
}

impl Resolution {
    pub fn from_popup(value: i32) -> Self {
        match value {
            2 => Self::R392,
            3 => Self::R518,
            _ => Self::R266,
        }
    }

    pub fn size(self) -> i32 {
        match self {
            Self::R266 => 266,
            Self::R392 => 392,
            Self::R518 => 518,
        }
    }
}

pub struct ModelCatalog {
    pub bundled_models: Vec<BundledModel>,
}

impl ModelCatalog {
    pub fn scan() -> Self {
        Self {
            bundled_models: scan_model_roots(&default_scan_roots()),
        }
    }
}

pub fn resolve_model_selection(
    model_popup: i32,
    custom_path: &str,
    bundled_models: &[BundledModel],
) -> (
    Option<std::path::PathBuf>,
    Option<ModelManifest>,
    Option<String>,
) {
    let custom = custom_path.trim();
    if !custom.is_empty() {
        let path = std::path::PathBuf::from(custom);
        return match load_manifest(&path) {
            Ok(manifest) => (Some(path), Some(manifest), None),
            Err(err) => (None, None, Some(err.to_string())),
        };
    }

    if bundled_models.is_empty() {
        return (
            None,
            None,
            Some(
                "no models found; add packs under MediaCore/DepthONNX/models or use Browse Model Folder"
                    .into(),
            ),
        );
    }

    let index = (model_popup - 1) as usize;
    if index >= bundled_models.len() {
        return (None, None, Some("invalid model selection".into()));
    }

    let selected = &bundled_models[index];
    (
        Some(selected.directory.clone()),
        Some(selected.manifest.clone()),
        None,
    )
}

pub struct SmoothingCache {
    inner: parking_lot::Mutex<std::collections::HashMap<u64, CacheSlot>>,
    max_bytes: usize,
    max_entries: usize,
}

struct CacheSlot {
    width: i32,
    depth: Vec<f32>,
}

impl SmoothingCache {
    pub fn remove(&self, key: u64) {
        self.inner.lock().remove(&key);
    }

    pub fn clear(&self) {
        // Replace the map to release its allocation as well as frame vectors.
        *self.inner.lock() = std::collections::HashMap::new();
    }

    pub fn new() -> Self {
        Self {
            inner: parking_lot::Mutex::new(std::collections::HashMap::new()),
            max_bytes: 256 * 1024 * 1024,
            max_entries: 256,
        }
    }

    pub fn blend(&self, key: u64, size: i32, depth: &mut [f32], alpha: f32) -> Result<(), after_effects::Error> {
        let mut guard = self.inner.lock();
        if !alpha.is_finite() || alpha <= 0.0 {
            guard.remove(&key);
            return Ok(());
        }
        let alpha = alpha.clamp(0.0, 0.99);
        if let Some(slot) = guard.get_mut(&key) {
            if alpha > 0.0 && slot.width == size && slot.depth.len() == depth.len() {
                for (out, prev) in depth.iter_mut().zip(slot.depth.iter()) {
                    *out = alpha * prev + (1.0 - alpha) * *out;
                }
                slot.depth.copy_from_slice(depth);
                return Ok(());
            }
        }
        // Reject growth without evicting another effect's temporal history or
        // modifying this frame. Capacity counts actual retained Vec storage.
        let error = after_effects::Error::OutOfMemory;
        if !guard.contains_key(&key) && guard.len() >= self.max_entries { return Err(error); }
        let retained = guard.iter().filter(|(k,_)| **k != key).try_fold(0usize, |sum,(_,slot)| {
            slot.depth.capacity().checked_mul(std::mem::size_of::<f32>()).and_then(|bytes|sum.checked_add(bytes))
        }).ok_or(error)?;
        let bytes = depth.len().checked_mul(std::mem::size_of::<f32>()).ok_or(error)?;
        if retained.checked_add(bytes).ok_or(error)? > self.max_bytes { return Err(error); }
        let mut stored = Vec::new();
        stored.try_reserve_exact(depth.len()).map_err(|_|error)?;
        let capacity_bytes = stored.capacity().checked_mul(std::mem::size_of::<f32>()).ok_or(error)?;
        if retained.checked_add(capacity_bytes).ok_or(error)? > self.max_bytes { return Err(error); }
        guard.try_reserve(1).map_err(|_|error)?;
        stored.extend_from_slice(depth);
        guard.insert(
            key,
            CacheSlot {
                width: size,
                depth: stored,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn teardown_releases_history_and_reused_key_starts_fresh() {
        let cache=SmoothingCache::new();
        cache.blend(1,2,&mut [0.25;4],0.5).unwrap();
        cache.blend(2,2,&mut [0.6;4],0.5).unwrap();
        cache.remove(1);
        cache.remove(1); // Repeated teardown is harmless.
        assert_eq!(cache.inner.lock().len(),1);
        let mut fresh=[0.9;4];
        cache.blend(1,2,&mut fresh,0.5).unwrap();
        assert_eq!(fresh,[0.9;4]);
        assert_eq!(cache.inner.lock().get(&2).unwrap().depth,[0.6;4]);
        cache.clear();
        assert!(cache.inner.lock().is_empty());
        assert_eq!(cache.inner.lock().capacity(),0);
    }
    #[test]
    fn disabled_smoothing_releases_history() {
        let cache=SmoothingCache::new();
        cache.blend(1,2,&mut [0.25;4],0.5).unwrap();
        let mut current=[0.75;4];
        cache.blend(1,2,&mut current,0.0).unwrap();
        assert_eq!(current,[0.75;4]);
        assert!(cache.inner.lock().is_empty());
    }
    #[test]
    fn same_shape_reuses_storage_and_resize_resets_history() {
        let cache=SmoothingCache::new();
        cache.blend(1,2,&mut [0.25;4],0.5).unwrap();
        let ptr=cache.inner.lock().get(&1).unwrap().depth.as_ptr();
        let mut current=[0.75;4];
        cache.blend(1,2,&mut current,0.5).unwrap();
        assert_eq!(current,[0.5;4]);
        assert_eq!(cache.inner.lock().get(&1).unwrap().depth.as_ptr(),ptr);
        let mut resized=[0.9];
        cache.blend(1,1,&mut resized,0.5).unwrap();
        assert_eq!(resized,[0.9]);
    }

    #[test]
    fn concurrent_instances_keep_independent_history_during_resize_and_release() {
        let mut cache = SmoothingCache::new();
        cache.max_entries = 8;
        cache.max_bytes = 8 * 7 * 7 * std::mem::size_of::<f32>();
        let start = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for key in 0..8u64 {
                let cache = &cache;
                let start = &start;
                scope.spawn(move || {
                    let mut previous: Option<(i32, Vec<f32>)> = None;
                    start.wait();
                    for frame in 0..200 {
                        let size = [2i32, 3, 5, 7][(frame / 5 + key as usize) % 4];
                        if frame % 17 == 0 {
                            cache.remove(key);
                            previous = None;
                        }
                        let mut image: Vec<f32> = (0..size * size)
                            .map(|pixel| ((key as i32 * 11 + frame as i32 + pixel) % 64) as f32 / 64.0)
                            .collect();
                        let mut expected = image.clone();
                        if let Some((old_size, old)) = &previous {
                            if *old_size == size {
                                for (value, prior) in expected.iter_mut().zip(old) {
                                    *value = (*value + prior) / 2.0;
                                }
                            }
                        }
                        cache.blend(key, size, &mut image, 0.5).unwrap();
                        assert_eq!(image, expected, "key={key} frame={frame}");
                        previous = Some((size, expected));
                        let guard = cache.inner.lock();
                        assert!(guard.len() <= cache.max_entries);
                        assert!(guard.values().map(|slot| slot.depth.capacity() * 4).sum::<usize>() <= cache.max_bytes);
                    }
                    cache.remove(key);
                });
            }
        });
        assert!(cache.inner.lock().is_empty());
        cache.clear();
        assert_eq!(cache.inner.lock().capacity(), 0);
    }

    #[test]
    fn history_limits_reject_growth_without_eviction_or_frame_changes() {
        let mut cache=SmoothingCache::new();
        cache.max_bytes=32;
        cache.max_entries=2;
        cache.blend(1,2,&mut [0.25;4],0.5).unwrap();
        cache.blend(2,2,&mut [0.75;4],0.5).unwrap();
        let mut frame=[0.9;4];
        assert!(cache.blend(3,2,&mut frame,0.5).is_err());
        assert_eq!(frame,[0.9;4]);
        assert_eq!(cache.inner.lock().len(),2);
        let mut larger=[0.8;5];
        assert!(cache.blend(1,5,&mut larger,0.5).is_err());
        assert_eq!(larger,[0.8;5]);
        assert_eq!(cache.inner.lock().get(&1).unwrap().depth,[0.25;4]);
        cache.blend(2,2,&mut frame,0.0).unwrap();
        cache.blend(1,5,&mut larger,0.5).unwrap();
        assert_eq!(larger,[0.8;5]);
        assert!(cache.inner.lock().values().map(|s|s.depth.capacity()*4).sum::<usize>()<=32);
    }
}
