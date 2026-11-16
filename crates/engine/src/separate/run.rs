//! A model run in ONNX Runtime: on the GPU and Neural Engine through Core ML (Apple silicon) or
//! DirectML (Windows), what they can't run on the CPU.

use ort::{
    environment::Environment,
    session::{Session, builder::AutoDevicePolicy},
    value::Tensor as Value,
};

use super::{About, Input, Model, Tensor};

pub struct Runner {
    session: Session,
    // Kept for as long as its session.
    _env: Environment,
}

/// The GPU's (and Neural Engine's) way in: Core ML on a Mac, DirectML on Windows.
fn gpu() -> ort::ep::ExecutionProviderDispatch {
    #[cfg(target_os = "macos")]
    {
        use ort::ep::coreml::{ComputeUnits, ModelFormat};
        // Compiled once, kept: a big model takes minutes to compile for Core ML.
        let cache = crate::paths::caches().join("Core ML");
        let _ = std::fs::create_dir_all(&cache);
        ort::ep::CoreML::default()
            .with_model_format(ModelFormat::MLProgram)
            .with_compute_units(ComputeUnits::All)
            .with_model_cache_dir(cache.display())
            .build()
    }
    #[cfg(windows)]
    ort::ep::DirectML::default().build()
}

/// A session for `file`: on the GPU where it can, else (`gpu` false) on the CPU alone. ONNX
/// Runtime would otherwise pick Core ML itself on a Mac, which some models can't compile for.
fn session(env: &Environment, file: &std::path::Path, gpu: bool) -> Result<Session, String> {
    let b = Session::builder(env).map_err(|e| e.to_string())?;
    let b = match gpu {
        true => b.with_execution_providers([self::gpu(), ort::ep::CPU::default().build()]),
        false => b.with_auto_device(AutoDevicePolicy::PreferCPU),
    };
    b.map_err(|e| e.to_string())?
        .commit_from_file(file)
        .map_err(|e| e.to_string())
}

impl Runner {
    pub fn new(model: &Model) -> Result<Runner, String> {
        let name = &model.about.name;
        let env = ort::init().build().map_err(|e| format!("{name}: {e}"))?;
        // On the GPU if its model.json asks, and the CPU if the GPU can't take it.
        let file = model.file();
        let session = match model.about.gpu {
            true => session(&env, &file, true).or_else(|_| session(&env, &file, false)),
            false => session(&env, &file, false),
        }
        .map_err(|e| format!("{name}: {e}"))?;
        let ins = session.inputs().len();
        let want = match model.about.input {
            Input::SpecSplit => 2,
            _ => 1,
        };
        if ins != want {
            return Err(format!(
                "{name} takes {ins} inputs, not the {want} its model.json's input ({:?}) does",
                model.about.input
            ));
        }
        Ok(Runner { session, _env: env })
    }

    /// Each input's and output's name and shape (-1 where it isn't fixed), on the CPU: to write
    /// a model.json from.
    pub fn inspect(file: &std::path::Path) -> Result<super::Shapes, String> {
        let env = ort::init().build().map_err(|e| e.to_string())?;
        let session = session(&env, file, false)?;
        let shapes = |o: &[ort::value::Outlet]| -> Vec<(String, Vec<i64>)> {
            o.iter()
                .map(|o| {
                    let shape = o.dtype().tensor_shape().map(|s| s.to_vec());
                    (o.name().to_string(), shape.unwrap_or_default())
                })
                .collect()
        };
        Ok((shapes(session.inputs()), shapes(session.outputs())))
    }

    /// Samples a run takes, as its first input's shape says (none where it isn't fixed).
    pub fn segment(&self, about: &About) -> Option<usize> {
        let shape = self.session.inputs().first()?.dtype().tensor_shape()?;
        let last = usize::try_from(*shape.last()?).ok().filter(|&n| n > 0)?;
        match about.input {
            Input::Wave => Some(last),
            _ => Some((last - 1) * about.hop),
        }
    }

    /// The model run on `inputs` (in its inputs' order): its outputs, in theirs.
    pub fn run(&mut self, inputs: Vec<Tensor>) -> Result<Vec<Tensor>, String> {
        let names: Vec<String> = self
            .session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        let mut values = Vec::new();
        for (name, (shape, data)) in names.into_iter().zip(inputs) {
            let v = Value::from_array((shape, data)).map_err(|e| e.to_string())?;
            values.push((std::borrow::Cow::from(name), v.into_dyn()));
        }
        let outs = self.session.run(values).map_err(|e| e.to_string())?;
        outs.values()
            .map(|v| {
                let (shape, data) = v.try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
                let shape = shape.iter().map(|&d| d.max(0) as usize).collect();
                Ok((shape, data.to_vec()))
            })
            .collect()
    }
}
