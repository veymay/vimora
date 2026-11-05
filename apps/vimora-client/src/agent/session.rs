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
    } = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut rd)).await??
    else {
        bail!("unexpected first message");
    };
    if version != PROTOCOL_VERSION {
        bail!("protocol mismatch: console v{version}, agent v{PROTOCOL_VERSION}");
    }
    let client_nonce = auth::nonce();
    let info = ClientInfo {
        hostname: system::hostname(),
        username: system::username(),
        os: std::env::consts::OS.into(),
        display_name: cfg.name.clone(),
        machine_id: system::machine_id(),
    };
    let mac = if cfg.key.is_empty() {
        match service::call(IpcReq::ClientMac {
            server_nonce: server_nonce.clone(),
            client_nonce: client_nonce.clone(),
        })
        .await?
        {
            IpcResp::Mac(m) => m,
            _ => bail!("Vimora xizmati noto'g'ri javob berdi"),
        }
    } else {
        auth::client_mac(&cfg.key, &server_nonce, &client_nonce)
    };
    write_msg(
        &mut wr,
        &ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            nonce: client_nonce.clone(),
            mac,
            info,
        },
    )
    .await?;
    let ServerMsg::Welcome {
        mac,
        stream: initial,
    } = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut rd))
        .await?
        .context("console rejected us (wrong connection key?)")?
    else {
        bail!("console rejected us (wrong connection key?)");
    };
    // The service (if any) verifies too, and remembers the session for signed commands.
    let by_service = matches!(
        service::call(IpcReq::VerifyServer {
            client_nonce: client_nonce.clone(),
            server_nonce: server_nonce.clone(),
            mac: mac.clone()
        })
        .await,
        Ok(IpcResp::Verified(true))
    );
    let ok = if cfg.key.is_empty() {
        by_service
    } else {
        auth::verify_server(&cfg.key, &client_nonce, &server_nonce, &mac)
    };
    if !ok {
        bail!("console failed authentication — ignoring it");
    }
    *ctx.session.lock().unwrap() = (client_nonce.clone(), 0);
    info!("authenticated with console {addr}");
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
            if write_msg(&mut wr, &m).await.is_err() {
                break;
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

    // --- main read loop ----------------------------------------------------------------------
    let result: Result<()> = async {
        loop {
            let msg: ServerMsg = tokio::time::timeout(Duration::from_secs(20), read_msg(&mut rd))
                .await
                .context("console timed out")??;
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
                    mac,
                } => {
                    let (ok, message) = match privileged(ctx, counter, payload, mac).await {
                        Ok(m) => (true, m),
                        Err(e) => (false, format!("{e:#}")),
                    };
                    let _ = tx.send(ClientMsg::CommandResult { ok, message }).await;
                    send_status(&tx);
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
            sync().await;
            None
        }
        Command::Unlock => {
            state.lock().unwrap().lock = None;
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

pub(super) fn spawn_self(args: &[&str]) -> Result<()> {
    std::process::Command::new(std::env::current_exe()?)
        .args(args)
        .spawn()?;
    Ok(())
}

/// Handles a console-signed privileged command: forwards it to the Vimora service, or (dev mode,
/// key in the agent config) verifies and runs it locally.
pub(super) async fn privileged(
    ctx: &Ctx,
    counter: u64,
    payload: Vec<u8>,
    mac: Vec<u8>,
) -> Result<String> {
    let cmd: PrivCommand = bincode::deserialize(&payload)?;
    // Logging off has to happen in the user's own session.
    if cmd == PrivCommand::Power(PowerAction::Logoff) {
        verify_locally_or_trust(ctx, counter, &payload, &mac)?;
        system::power(PowerAction::Logoff)?;
        return Ok("seansdan chiqarilmoqda".into());
    }
    let client_nonce = ctx.session.lock().unwrap().0.clone();
    let res = match service::call(IpcReq::Privileged {
        client_nonce,
        counter,
        payload: payload.clone(),
        mac: mac.clone(),
    })
    .await
    {
        Ok(IpcResp::Done { ok: true, message }) => Ok(message),
        Ok(IpcResp::Done { ok: false, message }) => Err(anyhow::anyhow!(message)),
        Ok(_) => Err(anyhow::anyhow!("Vimora xizmati noto'g'ri javob berdi")),
        Err(e) if !ctx.key.is_empty() => {
            // No service: run here (works only if the agent itself has the rights).
            verify_locally_or_trust(ctx, counter, &payload, &mac)?;
            let (engine, c) = (ctx.engine.clone(), cmd.clone());
            tokio::task::spawn_blocking(move || engine.execute(c))
                .await?
                .map_err(|err| anyhow::anyhow!("{err:#} (Vimora xizmati o'rnatilmagan: {e:#})"))
        }
        Err(e) => Err(e),
    };
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

pub(super) fn verify_locally_or_trust(
    ctx: &Ctx,
    counter: u64,
    payload: &[u8],
    mac: &[u8],
) -> Result<()> {
    if ctx.key.is_empty() {
        return Ok(()); // the console connection itself was authenticated by the service
    }
    let mut s = ctx.session.lock().unwrap();
    if counter <= s.1 || !auth::verify_priv(&ctx.key, &s.0, counter, payload, mac) {
        bail!("imzo noto'g'ri — buyruq rad etildi");
    }
    s.1 = counter;
    Ok(())
}
