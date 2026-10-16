//! Dictation: records the microphone while the mic button is on, then transcribes the audio
//! with a local Whisper model (whisper.cpp, Metal-accelerated). macOS only for now.
use std::path::PathBuf;

/// Whether dictation works on this platform; the mic button is hidden where it doesn't.
pub const AVAILABLE: bool = cfg!(target_os = "macos");

/// The Whisper model file. It is not bundled: download `ggml-base.bin` into this path.
pub fn model_path() -> PathBuf {
    crate::settings::config_dir().join("models").join("ggml-base.bin")
}

/// Where to get the model, for the error shown when it is missing.
pub const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";

/// Puts dictated text into the input at a character index, with a space before it when the text
/// before it doesn't end in whitespace. Returns the new input and the cursor after the text.
pub fn insert_at(input: &str, cursor: usize, text: &str) -> (String, usize) {
    let text = text.trim();
    let byte = input.char_indices().nth(cursor).map_or(input.len(), |(b, _)| b);
    let before = &input[..byte];
    if text.is_empty() {
        return (input.to_owned(), before.chars().count());
    }
    let space = if !before.is_empty() && !before.ends_with(char::is_whitespace) { " " } else { "" };
    let out = format!("{before}{space}{text}{}", &input[byte..]);
    (out, before.chars().count() + space.chars().count() + text.chars().count())
}

#[cfg(test)]
mod tests {
    use super::insert_at;

    #[test]
    fn inserts_into_empty_input_without_a_space() {
        assert_eq!(insert_at("", 0, "fix the test"), ("fix the test".to_owned(), 12));
    }

    #[test]
    fn adds_one_space_after_a_word_and_none_after_whitespace() {
        assert_eq!(insert_at("Please", 6, "refactor"), ("Please refactor".to_owned(), 15));
        assert_eq!(insert_at("Please ", 7, "refactor"), ("Please refactor".to_owned(), 15));
    }

    #[test]
    fn inserts_at_the_cursor_in_the_middle() {
        assert_eq!(insert_at("fix  now", 4, "it"), ("fix it now".to_owned(), 6));
        assert_eq!(insert_at("fix it", 6, "now"), ("fix it now".to_owned(), 10));
    }

    #[test]
    fn trims_text_and_ignores_empty_text() {
        assert_eq!(insert_at("a", 1, "  hello \n"), ("a hello".to_owned(), 7));
        assert_eq!(insert_at("a", 1, "   "), ("a".to_owned(), 1));
    }

    #[test]
    fn counts_characters_not_bytes() {
        // "ção" is 3 characters but 5 bytes; the cursor is after the 3rd character.
        assert_eq!(insert_at("ção", 3, "✨ x"), ("ção ✨ x".to_owned(), 7));
        assert_eq!(insert_at("ção", 1, "ok"), ("ç okão".to_owned(), 4));
    }

    #[test]
    fn cursor_past_the_end_appends() {
        assert_eq!(insert_at("ab", 99, "cd"), ("ab cd".to_owned(), 5));
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::model_path;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample};
    use std::sync::{Arc, Mutex};

    /// Whisper expects 16 kHz mono samples.
    const WHISPER_RATE: u32 = 16_000;

    /// An open microphone stream that collects mono samples until it is finished.
    pub struct Recorder {
        stream: cpal::Stream,
        samples: Arc<Mutex<Vec<f32>>>,
        rate: u32,
    }

    impl Recorder {
        pub fn start() -> Result<Self, String> {
            let device = cpal::default_host().default_input_device().ok_or("no microphone found")?;
            let supported = device.default_input_config().map_err(|e| format!("microphone: {e}"))?;
            let (rate, channels) = (supported.sample_rate(), supported.channels() as usize);
            let config = supported.config();
            let samples = Arc::new(Mutex::new(Vec::new()));
            let stream = match supported.sample_format() {
                SampleFormat::F32 => input_stream::<f32>(&device, &config, channels, &samples),
                SampleFormat::I16 => input_stream::<i16>(&device, &config, channels, &samples),
                SampleFormat::U16 => input_stream::<u16>(&device, &config, channels, &samples),
                other => Err(format!("unsupported microphone format {other:?}")),
            }?;
            stream.play().map_err(|e| format!("microphone: {e}"))?;
            Ok(Self { stream, samples, rate: rate.0 })
        }

        /// Stops recording and returns the audio as 16 kHz mono.
        pub fn finish(self) -> Vec<f32> {
            drop(self.stream);
            let raw = std::mem::take(&mut *self.samples.lock().unwrap_or_else(|e| e.into_inner()));
            resample(&raw, self.rate, WHISPER_RATE)
        }
    }

    /// Builds an input stream that averages the channels of each frame into one mono sample.
    fn input_stream<T: SizedSample>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channels: usize,
        samples: &Arc<Mutex<Vec<f32>>>,
    ) -> Result<cpal::Stream, String>
    where
        f32: FromSample<T>,
    {
        let samples = samples.clone();
        device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut out) = samples.lock() {
                        for frame in data.chunks(channels) {
                            let sum: f32 = frame.iter().map(|&s| f32::from_sample_(s)).sum();
                            out.push(sum / channels as f32);
                        }
                    }
                },
                |e| eprintln!("microphone error: {e}"),
                None,
            )
            .map_err(|e| format!("microphone: {e}"))
    }

    /// Linear resampling; plenty for speech.
    fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
        if from == to {
            return input.to_vec();
        }
        let step = from as f64 / to as f64;
        let len = (input.len() as f64 / step) as usize;
        (0..len)
            .map(|i| {
                let pos = i as f64 * step;
                let at = pos as usize;
                let a = input[at];
                let b = input.get(at + 1).copied().unwrap_or(a);
                a + (b - a) * (pos - at as f64) as f32
            })
            .collect()
    }

    /// Transcribes 16 kHz mono audio. Loads the model each time, so it runs on a worker thread.
    pub fn transcribe(samples: &[f32]) -> Result<String, String> {
        use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
        let path = model_path();
        let ctx = WhisperContext::new_with_params(&path.to_string_lossy(), WhisperContextParameters::default())
            .map_err(|e| format!("whisper: {e}"))?;
        let mut state = ctx.create_state().map_err(|e| format!("whisper: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // Auto-detects the language, so Portuguese and English both work.
        params.set_language(None);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        state.full(params, samples).map_err(|e| format!("whisper: {e}"))?;
        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(&segment.to_str_lossy().map_err(|e| format!("whisper: {e}"))?);
        }
        Ok(text.trim().to_owned())
    }

    #[cfg(test)]
    mod tests {
        use super::resample;

        #[test]
        fn resamples_to_the_target_length() {
            assert_eq!(resample(&[0.0; 48_000], 48_000, 16_000).len(), 16_000);
            assert_eq!(resample(&[0.5; 100], 16_000, 16_000), vec![0.5; 100]);
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::{Recorder, transcribe};

#[cfg(not(target_os = "macos"))]
pub struct Recorder;

#[cfg(not(target_os = "macos"))]
impl Recorder {
    pub fn start() -> Result<Self, String> {
        Err("dictation is only available on macOS for now".into())
    }

    pub fn finish(self) -> Vec<f32> {
        Vec::new()
    }
}

#[cfg(not(target_os = "macos"))]
pub fn transcribe(_samples: &[f32]) -> Result<String, String> {
    Err("dictation is only available on macOS for now".into())
}
