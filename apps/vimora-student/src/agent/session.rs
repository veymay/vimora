//! One authenticated teacher session: handshake, message loop and command execution.

use super::*;

pub(super) async fn session(
    cfg: &Config,
    addr: &str,
    ctx: &Ctx,
    ov_tx: &mpsc::Sender<OverlayReq>,
) -> Result<()> {
    let state = &ctx.state;
    let stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .context("connect timeout")??;
    stream.set_nodelay(true)?;
    let (mut rd, mut wr) = stream.into_split();

    // --- handshake -------------------------------------------------------------------------
    let TeacherMsg::Hello {
        version,
        nonce: teacher_nonce,
        teacher_key,
    } = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut rd)).await??
    else {
        bail!("unexpected first message");
    };
    if version < MIN_PROTOCOL_VERSION {
        bail!("protocol mismatch: teacher v{version}, agent v{PROTOCOL_VERSION}");
    }
    let pinned = *ctx.pin.lock().unwrap();
    if pinned.is_some_and(|k| k != teacher_key) {
        bail!(
            "boshqa o'qituvchi ({}) — bu kompyuter boshqa o'qituvchiga bog'langan",
            auth::fingerprint(&teacher_key)
        );
    }
    let student_nonce = auth::nonce();
    let info = StudentInfo {
        hostname: system::hostname(),
        username: system::username(),
        os: std::env::consts::OS.into(),
        display_name: cfg.name.clone(),
        machine_id: system::machine_id(),
    };
    write_msg(
        &mut wr,
        &StudentMsg::Hello {
            version: PROTOCOL_VERSION,
            nonce: student_nonce.clone(),
            info,
        },
    )
    .await?;
    let TeacherMsg::Welcome {
        sig,
        stream: initial,
    } = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut rd)).await??
    else {
        bail!("unexpected welcome");
    };
    // The teacher must prove it owns the key it announced (no impersonation of a paired teacher).
    if !auth::verify_welcome(&teacher_key, &student_nonce, &teacher_nonce, &sig) {
        bail!("o'qituvchi imzosi noto'g'ri — e'tiborsiz qoldirildi");
    }
    // The service pairs on first contact (and refuses a different teacher afterwards).
    match service::call(IpcReq::VerifyServer {
        teacher_key,
        student_nonce: student_nonce.clone(),
        teacher_nonce: teacher_nonce.clone(),
        sig: sig.clone(),
    })
    .await
    {
        Ok(IpcResp::Verified(true)) | Err(_) => {}
        Ok(_) => bail!("Vimora xizmati bu o'qituvchini rad etdi (boshqa o'qituvchiga bog'langan)"),
    }
    if pinned.is_none() {
        info!("paired with teacher {}", auth::fingerprint(&teacher_key));
        crate::config::save_pin(&teacher_key);
        crate::config::set_released(false);
        *ctx.pin.lock().unwrap() = Some(teacher_key);
    }
    *ctx.session.lock().unwrap() = (student_nonce.clone(), 0);
    info!("authenticated with teacher {addr}");
    crate::config::save_last_teacher(addr);
    {
        let mut s = state.lock().unwrap();
        s.connected = true;
        s.disconnected_at = None;
        if let Some(n) = super::TEACHER_NAMES.lock().unwrap().get(addr) {
            s.teacher = Some(n.clone());
        }
    }

    // --- writer ----------------------------------------------------------------------------
    let (tx, mut rx) = mpsc::channel::<StudentMsg>(32);
    *ctx.uplink.lock().unwrap() = Some(tx.clone());
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            // A teacher that stops reading must not freeze the agent: give up and reconnect.
            match tokio::time::timeout(Duration::from_secs(20), write_msg(&mut wr, &m)).await {
                Ok(Ok(())) => {}
                _ => break,
            }
        }
    });

    // --- screen capture thread ---------------------------------------------------------------
    let stream_cfg = Arc::new(Mutex::new(initial));
    let stop = Arc::new(AtomicBool::new(false));
    {
        let (cfg, stop, tx) = (stream_cfg.clone(), stop.clone(), tx.clone());
        std::thread::spawn(move || capture_loop(cfg, stop, tx));
    }

    // --- remote-control thread -------------------------------------------------------------
    let (in_tx, in_rx) = std::sync::mpsc::channel::<vimora_core::InputEvent>();
    std::thread::spawn(move || {
        let mut inj: Option<input::Injector> = None;
        while let Ok(ev) = in_rx.recv() {
            if inj.is_none() {
                match input::Injector::new() {
                    Ok(i) => inj = Some(i),
                    Err(e) => {
                        warn!("remote control unavailable: {e:#}");
                        continue;
                    }
                }
            }
            if let Some(i) = &mut inj {
                if let Err(e) = i.inject(&ev) {
                    warn!("inject: {e:#}");
                }
            }
        }
    });

    let send_status = |tx: &mpsc::Sender<StudentMsg>| {
        let st = state.lock().unwrap().status();
        let _ = tx.try_send(StudentMsg::Status(st));
    };
    send_status(&tx);
    let _ = tx.try_send(StudentMsg::AgentVersion(env!("CARGO_PKG_VERSION").into()));

    // --- main read loop ----------------------------------------------------------------------
    let result: Result<()> = async {
        loop {
            let msg: TeacherMsg =
                match tokio::time::timeout(Duration::from_secs(20), read_msg(&mut rd))
                    .await
                    .context("teacher timed out")?
                {
                    Ok(m) => m,
                    // A newer teacher's message this agent doesn't know: skip it.
                    Err(e) if vimora_core::net::is_undecodable(&e) => continue,
                    Err(e) => return Err(e),
                };
            match msg {
                TeacherMsg::Ping => {
                    let _ = tx.try_send(StudentMsg::Pong);
                }
                TeacherMsg::Input(ev) => {
                    let _ = in_tx.send(ev);
                }
                TeacherMsg::ShowFrame { jpeg, .. } => {
                    let _ = ov_tx.try_send(OverlayReq::Frame(jpeg));
                }
                TeacherMsg::Command(Command::GetSecurityInfo) => {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Ok(info) = tokio::task::spawn_blocking(crate::secinfo::collect).await
                        {
                            let _ = tx.send(StudentMsg::SecurityInfo(info)).await;
                        }
                    });
                }
                TeacherMsg::Command(Command::CollectFiles) => {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Ok(files) =
                            tokio::task::spawn_blocking(system::collect_assignments).await
                        {
                            let _ = tx.send(StudentMsg::Collected(files)).await;
                        }
                    });
                }
                TeacherMsg::Command(Command::GetSystemInfo) => {
                    // Sampled off the async threads (the first sample waits a moment).
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Ok(info) = tokio::task::spawn_blocking(crate::sysmon::snapshot).await
                        {
                            let _ = tx.send(StudentMsg::SystemInfo(info)).await;
                        }
                    });
                }
                TeacherMsg::Command(cmd) => {
                    if let Some(res) = handle_command(cmd, ctx, ov_tx, &stream_cfg).await {
                        let (ok, message) = match res {
                            Ok(m) => (true, m),
                            Err(e) => (false, format!("{e:#}")),
                        };
                        let _ = tx.send(StudentMsg::CommandResult { ok, message }).await;
                    }
                    send_status(&tx);
                }
                TeacherMsg::Privileged {
                    counter,
                    payload,
                    sig,
                } => {
                    let (ok, message) = match privileged(ctx, counter, payload, sig).await {
                        Ok(m) => (true, m),
                        Err(e) => (false, format!("{e:#}")),
                    };
                    let _ = tx.send(StudentMsg::CommandResult { ok, message }).await;
                    send_status(&tx);
                    // Removed from this teacher: say goodbye (the result above still goes out).
                    if *UNPAIRED.lock().unwrap() {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        bail!("o'qituvchidan ajratildi");
                    }
                }
                TeacherMsg::Hello { .. } | TeacherMsg::Welcome { .. } => {}
            }
        }
    }
    .await;

    stop.store(true, Ordering::Relaxed);
    writer.abort();
    result
}

/// Executes a command. Returns `Some(result)` for commands whose outcome is reported back.
pub(super) async fn handle_command(
    cmd: Command,
    ctx: &Ctx,
    ov_tx: &mpsc::Sender<OverlayReq>,
    stream_cfg: &Arc<Mutex<StreamConfig>>,
) -> Option<Result<String>> {
    let state = &ctx.state;
    let sync = || async {
        let _ = ov_tx.send(OverlayReq::Sync).await;
    };
    match cmd {
        Command::TeacherInfo { name } => {
            state.lock().unwrap().teacher = Some(name).filter(|n| !n.trim().is_empty());
            None
        }
        Command::Lock { title, message } => {
            state.lock().unwrap().lock = Some((title, message));
            save_lock(state);
            sync().await;
            None
        }
        Command::Unlock => {
            {
                let mut s = state.lock().unwrap();
                s.lock = None;
                s.unlock = None;
            }
            save_lock(state);
            sync().await;
            None
        }
        Command::LockStyle { icon } => {
            state.lock().unwrap().lock_icon = icon;
            save_lock(state);
            sync().await;
            None
        }
        Command::LockLook(look) => {
            state.lock().unwrap().lock_look = look;
            save_lock(state);
            sync().await;
            None
        }
        Command::UnlockCode { salt, hash, digits } => {
            state.lock().unwrap().unlock =
                (!hash.is_empty()).then_some((salt, hash, digits.clamp(4, 8)));
            save_lock(state);
            sync().await;
            None
        }
        Command::ShowStart => {
            state.lock().unwrap().show = true;
            sync().await;
            None
        }
        Command::ShowStop => {
            state.lock().unwrap().show = false;
            sync().await;
            None
        }
        Command::SetStream(c) => {
            // Live (full-size, fast) = the teacher is watching: say so on screen.
            let live = c.interval_ms > 0 && c.interval_ms <= 300 && c.max_width >= 1000;
            *stream_cfg.lock().unwrap() = c;
            crate::watched::set(live);
            None
        }
        Command::InputLock { keyboard, mouse } => Some(crate::inputlock::set(keyboard, mouse)),
        Command::ClearHelp => {
            state.lock().unwrap().help = false;
            None
        }
        Command::SetAppAllowlist(list) => {
            let n = list.len();
            state.lock().unwrap().allow_apps = list;
            save_apps(state);
            Some(Ok(if n == 0 {
                "dasturlar: oq ro'yxat o'chirildi".to_string()
            } else if cfg!(windows) {
                format!("faqat {n} ta dasturga ruxsat")
            } else {
                "oq ro'yxat faqat Windows'da ishlaydi".to_string()
            }))
        }
        Command::SetAppBlocklist(list) => {
            let n = list.len();
            state.lock().unwrap().apps = list;
            save_apps(state);
            Some(Ok(format!("{n} ta dastur bloklandi")))
        }
        Command::SetNetPolicy(_) => Some(Err(anyhow::anyhow!(
            "eski o'qituvchi versiyasi: o'qituvchini yangilang"
        ))),
        Command::Message { text, .. } => {
            Some(chat_supervisor::deliver(ctx, &text).map(|_| "xabar yetkazildi".into()))
        }
        Command::ChatEnd => {
            chat_supervisor::end(ctx);
            None
        }
        Command::KillProcess(pid) => Some(crate::sysmon::kill(pid)),
        Command::OpenFile(name) => Some(system::open_received(&name)),
        Command::Poll {
            id,
            question,
            options,
        } => {
            lesson_supervisor::poll(ctx, id, question, options);
            None
        }
        Command::PollEnd => {
            lesson_supervisor::poll_end();
            None
        }
        Command::LessonBanner { goal, end_unix } => {
            lesson_supervisor::banner(goal, end_unix);
            None
        }
        Command::StartTest(def) => {
            lesson_supervisor::test(ctx, def);
            None
        }
        Command::EndTest => {
            lesson_supervisor::test_end();
            None
        }
        // Answered in the session loop (it needs the outgoing channel).
        Command::CollectFiles | Command::GetSecurityInfo => None,
        // Answered in the session loop (it needs the outgoing channel).
        Command::GetSystemInfo => None,
        Command::OpenUrl(url) => Some(system::open_url(&url).map(|_| format!("ochildi: {url}"))),
        Command::Run { program, args } => {
            Some(system::run_program(&program, &args).map(|_| format!("ishga tushdi: {program}")))
        }
        Command::Power(a) => Some(system::power(a).map(|_| format!("{a:?}"))),
        Command::InstallUpdate {
            name,
            data,
            sums,
            sig,
        } => Some(install_from_teacher(name, data, sums, sig).await),
        Command::SendFile { name, data } => Some(
            tokio::task::spawn_blocking(move || system::save_file(&name, &data))
                .await
                .unwrap()
                .map(|p| format!("saqlandi: {}", p.display())),
        ),
    }
}

/// A Vimora installer from the teacher: saved to a temporary file and handed to the service,
/// which checks the signature itself (the agent is not trusted with that).
async fn install_from_teacher(
    name: String,
    data: Vec<u8>,
    sums: String,
    sig: String,
) -> anyhow::Result<String> {
    // Only a plain file name: nothing outside the folder.
    let name = std::path::Path::new(&name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| anyhow::anyhow!("fayl nomi noto'g'ri"))?;
    let dir = std::env::temp_dir().join("vimora-lan-update");
    let path = dir.join(&name);
    let p2 = path.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&p2, &data)
    })
    .await??;
    let res = service::call(IpcReq::InstallUpdate {
        path: path.to_string_lossy().into_owned(),
        sums,
        sig,
    })
    .await;
    let _ = std::fs::remove_file(&path);
    match res? {
        IpcResp::Done { ok: true, message } => Ok(message),
        IpcResp::Done { ok: false, message } => Err(anyhow::anyhow!(message)),
        _ => Err(anyhow::anyhow!("Vimora xizmati noto'g'ri javob berdi")),
    }
}

/// Handles a teacher-signed privileged command: forwards it to the Vimora service, or (dev mode,
/// no service running) verifies it against the paired teacher and runs it locally.
pub(super) async fn privileged(
    ctx: &Ctx,
    counter: u64,
    payload: Vec<u8>,
    sig: Vec<u8>,
) -> Result<String> {
    let cmd: PrivCommand = bincode::deserialize(&payload)?;
    // Sound belongs to the user's session: the agent mutes it, the service does the rest.
    // (Only the signature is checked here; the counter is checked where the command runs.)
    if let PrivCommand::SetDevicePolicy(p) = &cmd {
        let genuine = {
            let key = *ctx.pin.lock().unwrap();
            let s = ctx.session.lock().unwrap();
            key.is_some_and(|k| auth::verify_priv(&k, &s.0, counter, &payload, &sig))
        };
        if genuine {
            let off = p.sound_off;
            let _ = tokio::task::spawn_blocking(move || crate::audio::set_off(off)).await;
        }
    }
    // Logging off has to happen in the user's own session.
    if cmd == PrivCommand::Power(PowerAction::Logoff) {
        verify_locally(ctx, counter, &payload, &sig)?;
        system::power(PowerAction::Logoff)?;
        return Ok("seansdan chiqarilmoqda".into());
    }
    let student_nonce = ctx.session.lock().unwrap().0.clone();
    let res = match service::call(IpcReq::Privileged {
        student_nonce,
        counter,
        payload: payload.clone(),
        sig: sig.clone(),
    })
    .await
    {
        Ok(IpcResp::Done { ok: true, message }) => Ok(message),
        Ok(IpcResp::Done { ok: false, message }) => Err(anyhow::anyhow!(message)),
        Ok(_) => Err(anyhow::anyhow!("Vimora xizmati noto'g'ri javob berdi")),
        Err(e) => {
            // No service: run here (works only if the agent itself has the rights).
            verify_locally(ctx, counter, &payload, &sig)?;
            let (engine, c) = (ctx.engine.clone(), cmd.clone());
            tokio::task::spawn_blocking(move || engine.execute(c))
                .await?
                .map_err(|err| anyhow::anyhow!("{err:#} (Vimora xizmati o'rnatilmagan: {e:#})"))
        }
    };
    if cmd == PrivCommand::Unpair {
        // With or without a service: forget the teacher on the agent side too.
        if res.is_err() {
            verify_locally(ctx, counter, &payload, &sig)?;
        }
        let _ = crate::config::forget_pin();
        // The teacher's address is kept: the free PC tells it "I am free" directly, even where
        // broadcasts do not get through.
        // Free now: listed in every teacher until a teacher adds it.
        crate::config::set_released(true);
        *ctx.pin.lock().unwrap() = None;
        *UNPAIRED.lock().unwrap() = true;
        return Ok("Kompyuter o'qituvchidan ajratildi".into());
    }
    if res.is_ok() && matches!(cmd, PrivCommand::ListChanges | PrivCommand::CommitPaths(_)) {
        // The overlay content changed: report it to the teacher.
        let list = match service::call(IpcReq::Changes).await {
            Ok(IpcResp::Changes(l)) => l,
            _ => ctx.engine.changes(),
        };
        if let (Some(list), Some(tx)) = (list, ctx.uplink.lock().unwrap().as_ref()) {
            let _ = tx.try_send(StudentMsg::Changes(list));
        }
    }
    if res.is_ok() {
        if let PrivCommand::SetNetPolicy(p) = cmd {
            // The keyword watcher runs here, in the user's session.
            let mut p = p;
            p.keywords = p
                .keywords
                .iter()
                .map(|k| netcontrol::normalize(k))
                .filter(|k| !k.is_empty())
                .collect();
            ctx.state.lock().unwrap().net = p;
        }
    }
    res
}

pub(super) fn verify_locally(ctx: &Ctx, counter: u64, payload: &[u8], sig: &[u8]) -> Result<()> {
    let Some(key) = *ctx.pin.lock().unwrap() else {
        bail!("kompyuter hali o'qituvchiga bog'lanmagan");
    };
    let mut s = ctx.session.lock().unwrap();
    if counter <= s.1 || !auth::verify_priv(&key, &s.0, counter, payload, sig) {
        bail!("imzo noto'g'ri — buyruq rad etildi");
    }
    s.1 = counter;
    Ok(())
}
