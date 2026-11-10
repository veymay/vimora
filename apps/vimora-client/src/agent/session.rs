//! One authenticated console session: handshake, message loop and command execution.

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
    let ServerMsg::Hello {
        version,
        nonce: server_nonce,
        console_key,
    } = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut rd)).await??
    else {
        bail!("unexpected first message");
    };
    if version < MIN_PROTOCOL_VERSION {
        bail!("protocol mismatch: console v{version}, agent v{PROTOCOL_VERSION}");
    }
    let pinned = *ctx.pin.lock().unwrap();
    if pinned.is_some_and(|k| k != console_key) {
        bail!(
            "boshqa konsol ({}) — bu kompyuter boshqa konsolga bog'langan",
            auth::fingerprint(&console_key)
        );
    }
    let client_nonce = auth::nonce();
    let info = ClientInfo {
        hostname: system::hostname(),
        username: system::username(),
        os: std::env::consts::OS.into(),
        display_name: cfg.name.clone(),
        machine_id: system::machine_id(),
    };
    write_msg(
        &mut wr,
        &ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            nonce: client_nonce.clone(),
            info,
        },
    )
    .await?;
    let ServerMsg::Welcome {
        sig,
        stream: initial,
    } = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut rd)).await??
    else {
        bail!("unexpected welcome");
    };
    // The console must prove it owns the key it announced (no impersonation of a paired console).
    if !auth::verify_welcome(&console_key, &client_nonce, &server_nonce, &sig) {
        bail!("konsol imzosi noto'g'ri — e'tiborsiz qoldirildi");
    }
    // The service pairs on first contact (and refuses a different console afterwards).
    match service::call(IpcReq::VerifyServer {
        console_key,
        client_nonce: client_nonce.clone(),
        server_nonce: server_nonce.clone(),
        sig: sig.clone(),
    })
    .await
    {
        Ok(IpcResp::Verified(true)) | Err(_) => {}
        Ok(_) => bail!("Vimora xizmati bu konsolni rad etdi (boshqa konsolga bog'langan)"),
    }
    if pinned.is_none() {
        info!("paired with console {}", auth::fingerprint(&console_key));
        crate::config::save_pin(&console_key);
        *ctx.pin.lock().unwrap() = Some(console_key);
    }
    *ctx.session.lock().unwrap() = (client_nonce.clone(), 0);
    info!("authenticated with console {addr}");
    crate::config::save_last_console(addr);
    {
        let mut s = state.lock().unwrap();
        s.connected = true;
        s.disconnected_at = None;
    }

    // --- writer ----------------------------------------------------------------------------
    let (tx, mut rx) = mpsc::channel::<ClientMsg>(32);
    *ctx.uplink.lock().unwrap() = Some(tx.clone());
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            // A console that stops reading must not freeze the agent: give up and reconnect.
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

    let send_status = |tx: &mpsc::Sender<ClientMsg>| {
        let st = state.lock().unwrap().status();
        let _ = tx.try_send(ClientMsg::Status(st));
    };
    send_status(&tx);
    let _ = tx.try_send(ClientMsg::AgentVersion(env!("CARGO_PKG_VERSION").into()));

    // --- main read loop ----------------------------------------------------------------------
    let result: Result<()> = async {
        loop {
            let msg: ServerMsg =
                match tokio::time::timeout(Duration::from_secs(20), read_msg(&mut rd))
                    .await
                    .context("console timed out")?
                {
                    Ok(m) => m,
                    // A newer console's message this agent doesn't know: skip it.
                    Err(e) if vimora_core::net::is_undecodable(&e) => continue,
                    Err(e) => return Err(e),
                };
            match msg {
                ServerMsg::Ping => {
                    let _ = tx.try_send(ClientMsg::Pong);
                }
                ServerMsg::Input(ev) => {
                    let _ = in_tx.send(ev);
                }
                ServerMsg::ShowFrame { jpeg, .. } => {
                    let _ = ov_tx.try_send(OverlayReq::Frame(jpeg));
                }
                ServerMsg::Command(Command::GetSystemInfo) => {
                    // Sampled off the async threads (the first sample waits a moment).
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Ok(info) = tokio::task::spawn_blocking(crate::sysmon::snapshot).await
                        {
                            let _ = tx.send(ClientMsg::SystemInfo(info)).await;
                        }
                    });
                }
                ServerMsg::Command(cmd) => {
                    if let Some(res) = handle_command(cmd, ctx, ov_tx, &stream_cfg).await {
                        let (ok, message) = match res {
                            Ok(m) => (true, m),
                            Err(e) => (false, format!("{e:#}")),
                        };
                        let _ = tx.send(ClientMsg::CommandResult { ok, message }).await;
                    }
                    send_status(&tx);
                }
                ServerMsg::Privileged {
                    counter,
                    payload,
                    sig,
                } => {
                    let (ok, message) = match privileged(ctx, counter, payload, sig).await {
                        Ok(m) => (true, m),
                        Err(e) => (false, format!("{e:#}")),
                    };
                    let _ = tx.send(ClientMsg::CommandResult { ok, message }).await;
                    send_status(&tx);
                    // Removed from this console: say goodbye (the result above still goes out).
                    if *UNPAIRED.lock().unwrap() {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        bail!("konsoldan ajratildi");
                    }
                }
                ServerMsg::Hello { .. } | ServerMsg::Welcome { .. } => {}
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
            *stream_cfg.lock().unwrap() = c;
            None
        }
        Command::ClearHelp => {
            state.lock().unwrap().help = false;
            None
        }
        Command::SetAppBlocklist(list) => {
            let n = list.len();
            state.lock().unwrap().apps = list;
            Some(Ok(format!("{n} ta dastur bloklandi")))
        }
        Command::SetNetPolicy(_) => Some(Err(anyhow::anyhow!(
            "eski konsol versiyasi: konsolni yangilang"
        ))),
        Command::Message { text, .. } => {
            Some(chat_supervisor::deliver(ctx, &text).map(|_| "xabar yetkazildi".into()))
        }
        Command::ChatEnd => {
            chat_supervisor::end(ctx);
            None
        }
        Command::KillProcess(pid) => Some(crate::sysmon::kill(pid)),
        // Answered in the session loop (it needs the outgoing channel).
        Command::GetSystemInfo => None,
        Command::OpenUrl(url) => Some(system::open_url(&url).map(|_| format!("ochildi: {url}"))),
        Command::Run { program, args } => {
            Some(system::run_program(&program, &args).map(|_| format!("ishga tushdi: {program}")))
        }
        Command::Power(a) => Some(system::power(a).map(|_| format!("{a:?}"))),
        Command::SendFile { name, data } => Some(
            tokio::task::spawn_blocking(move || system::save_file(&name, &data))
                .await
                .unwrap()
                .map(|p| format!("saqlandi: {}", p.display())),
        ),
    }
}

/// Handles a console-signed privileged command: forwards it to the Vimora service, or (dev mode,
/// no service running) verifies it against the paired console and runs it locally.
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
    let client_nonce = ctx.session.lock().unwrap().0.clone();
    let res = match service::call(IpcReq::Privileged {
        client_nonce,
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
        // With or without a service: forget the console on the agent side too.
        if res.is_err() {
            verify_locally(ctx, counter, &payload, &sig)?;
        }
        let _ = crate::config::forget_pin();
        crate::config::save_last_console("");
        *ctx.pin.lock().unwrap() = None;
        *UNPAIRED.lock().unwrap() = true;
        return Ok("Kompyuter konsoldan ajratildi".into());
    }
    if res.is_ok() && matches!(cmd, PrivCommand::ListChanges | PrivCommand::CommitPaths(_)) {
        // The overlay content changed: report it to the console.
        let list = match service::call(IpcReq::Changes).await {
            Ok(IpcResp::Changes(l)) => l,
            _ => ctx.engine.changes(),
        };
        if let (Some(list), Some(tx)) = (list, ctx.uplink.lock().unwrap().as_ref()) {
            let _ = tx.try_send(ClientMsg::Changes(list));
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
        bail!("kompyuter hali konsolga bog'lanmagan");
    };
    let mut s = ctx.session.lock().unwrap();
    if counter <= s.1 || !auth::verify_priv(&key, &s.0, counter, payload, sig) {
        bail!("imzo noto'g'ri — buyruq rad etildi");
    }
    s.1 = counter;
    Ok(())
}
