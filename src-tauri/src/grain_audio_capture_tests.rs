// Included in the recorder tests to exercise private capture without public test hooks.
#[test]
fn batch_and_live_frames_keep_unenhanced_audio_through_stop_and_flush() {
    // Quiet speech plus DC exposes both high-pass filtering and stop-time
    // gain boosts. Compare both consumers with ordinary resampling only.
    for sample_rate in [16_000, 48_000] {
        let chunks: Vec<Vec<f32>> = [6000, 600, 137]
            .into_iter()
            .map(|len| {
                (0..len)
                    .map(|n| {
                        0.01 + 0.015
                            * (2.0 * std::f32::consts::PI * 200.0 * n as f32 / sample_rate as f32)
                                .sin()
                    })
                    .collect()
            })
            .collect();
        let mut expected = Vec::new();
        let mut resampler =
            super::FrameResampler::new(sample_rate as usize, 16_000, Duration::from_millis(30));
        for chunk in &chunks {
            resampler.push(chunk, |frame| expected.extend_from_slice(frame));
        }
        resampler.finish(|frame| expected.extend_from_slice(frame));

        let (sample_tx, sample_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (live_tx, live_rx) = mpsc::sync_channel(1);
        let streamed = Arc::new(Mutex::new(Vec::new()));
        let streamed_cb = streamed.clone();
        let worker = std::thread::spawn(move || {
            run_consumer(
                sample_rate,
                None,
                sample_rx,
                cmd_rx,
                None,
                None,
                Some(Arc::new(move |frame, speech| {
                    assert_eq!(speech, None);
                    streamed_cb.lock().unwrap().extend_from_slice(frame);
                    let _ = live_tx.try_send(());
                })),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
                Instant::now(),
            );
        });
        cmd_tx
            .send(Cmd::Start(VadPolicy::Disabled, Instant::now(), true))
            .unwrap();
        sample_tx
            .send(AudioChunk::Samples(chunks[0].clone()))
            .unwrap();
        live_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (reply_tx, reply_rx) = mpsc::channel();
        cmd_tx.send(Cmd::Stop(reply_tx)).unwrap();
        for chunk in &chunks[1..] {
            sample_tx.send(AudioChunk::Samples(chunk.clone())).unwrap();
        }
        sample_tx.send(AudioChunk::EndOfStream).unwrap();
        let batch = reply_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        drop(sample_tx);
        worker.join().unwrap();

        assert_eq!(batch, expected, "Batch at {sample_rate} Hz");
        assert_eq!(
            *streamed.lock().unwrap(),
            expected,
            "Live at {sample_rate} Hz"
        );
    }
}
