use super::super::*;

pub(crate) fn connections_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let s = strings(current.locale);
    let navigator = use_navigator();
    let active_id = current.snapshot.active_connection_id.clone();
    let lifecycle = current.snapshot.lifecycle;
    let connections = current.snapshot.connections.clone();
    let empty = connections.is_empty();

    let actions = rsx! {
        FlatButton {
            variant: FlatButtonVariant::Ghost,
            size: ButtonSize::Icon,
            onclick: move |_| {
                navigator.push(Route::ConnectionEditor { id: String::new() });
            },
            {arkit::icon("plus", 20.0, text_color())}
        }
    };

    let body = rsx! {
        column {
            width: "100%",
            align_items: "stretch",
            if empty {
                column {
                    width: "100%",
                    padding: 28.0,
                    align_items: "center",
                    background_color: surface(),
                    border_width: 1.0,
                    border_color: line(),
                    border_radius: 12.0,
                    {arkit::icon("server", 28.0, subtle())}
                    text {
                        content: s.empty_connections,
                        margin_top: 12.0,
                        font_size: 14.0,
                        font_color: subtle(),
                        text_align: "center",
                    }
                }
            } else {
                {connections.into_iter().map(|connection| {
                    let selected = active_id.as_deref() == Some(connection.id.as_str());
                    let is_live = selected && lifecycle.is_active();
                    let locked = selected && (lifecycle.is_active() || lifecycle.is_busy());
                    let key = connection.id.clone();
                    rsx! {
                        ConnectionCard {
                            key: "{key}",
                            state,
                            connection,
                            selected,
                            is_live,
                            locked,
                        }
                    }
                })}
            }
        }
    };

    scaffold(state, Route::Connections {}, actions, body)
}

/// Flat list card — avoids nested full-width buttons that clip children on ArkUI.
#[component]
fn ConnectionCard(
    state: Signal<State>,
    connection: VpnConnection,
    selected: bool,
    is_live: bool,
    locked: bool,
) -> Element {
    let s = strings(state.read().locale);
    let navigator = use_navigator();
    let id = connection.id.clone();
    let id_for_edit = connection.id.clone();
    let id_for_delete = connection.id.clone();
    let id_for_favorite = connection.id.clone();
    let name = connection.name.clone();
    let server = connection.server.clone();
    let group = if connection.group.is_empty() {
        "—".to_owned()
    } else {
        connection.group.clone()
    };
    let protocol = connection.protocol.as_label().to_owned();
    let favorite = connection.favorite;
    let auth_summary = connection.summary_auth();

    rsx! {
        column {
            width: "100%",
            margin_bottom: 12.0,
            background_color: surface(),
            border_width: if selected { 2.0 } else { 1.0 },
            border_color: if selected { accent() } else { line() },
            border_radius: 12.0,
            button {
                width: "100%",
                height: 92.0,
                background_color: if selected { muted() } else { surface() },
                border_width: 0.0,
                border_radius: 12.0,
                padding_top: 14.0,
                padding_right: 14.0,
                padding_bottom: 10.0,
                padding_left: 14.0,
                alignment: "top_start",
                onclick: move |_| dispatch(state, Action::SelectConnection(id.clone())),
                column {
                    width: "100%",
                    align_items: "start",
                    row {
                        width: "100%",
                        align_items: "center",
                        column {
                            layout_weight: 1.0,
                            align_items: "start",
                            clip: true,
                            text {
                                content: name,
                                width: "100%",
                                font_size: 16.0,
                                font_weight: 700,
                                font_color: text_color(),
                                max_lines: 1_i32,
                                text_overflow: "ellipsis",
                            }
                            text {
                                content: server,
                                width: "100%",
                                margin_top: 3.0,
                                font_size: 13.0,
                                font_color: subtle(),
                                max_lines: 1_i32,
                                text_overflow: "ellipsis",
                            }
                        }
                        row { width: 8.0 }
                        if is_live {
                            Badge { content: s.connected.to_owned(), variant: BadgeVariant::Default }
                        } else if selected {
                            Badge { content: s.current.to_owned(), variant: BadgeVariant::Secondary }
                        } else if favorite {
                            Badge { content: s.favorite.to_owned(), variant: BadgeVariant::Secondary }
                        }
                    }
                    row { height: 8.0 }
                    text {
                        content: format!("{protocol} · {group} · {auth_summary}"),
                        width: "100%",
                        font_size: 12.0,
                        font_color: subtle(),
                        max_lines: 1_i32,
                        text_overflow: "ellipsis",
                    }
                }
            }
            Separator {}
            row {
                width: "100%",
                height: 48.0,
                padding_left: 6.0,
                padding_right: 6.0,
                align_items: "center",
                FlatButton {
                    variant: FlatButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    onclick: move |_| dispatch(state, Action::ToggleFavorite(id_for_favorite.clone())),
                    {arkit::icon(if favorite { "star" } else { "star-off" }, 18.0, if favorite { warning() } else { subtle() })}
                }
                row { layout_weight: 1.0 }
                FlatButton {
                    variant: FlatButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    disabled: Some(locked),
                    onclick: move |_| {
                        navigator.push(Route::ConnectionEditor { id: id_for_edit.clone() });
                    },
                    {arkit::icon("pen-line", 18.0, text_color())}
                }
                FlatButton {
                    variant: FlatButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    disabled: Some(locked),
                    onclick: move |_| dispatch(state, Action::DeleteConnection(id_for_delete.clone())),
                    {arkit::icon("trash-2", 18.0, danger())}
                }
            }
        }
    }
}

/// Connection editor: common fields always visible; advanced behind a toggle.
/// Enum-like options use [`Select`] instead of radio groups.
pub(crate) fn connection_editor_page(state: Signal<State>, id: String) -> Element {
    let current = state.read().clone();
    let s = strings(current.locale);
    let navigator = use_navigator();
    let seed_id = id.clone();
    use_effect(move || {
        dispatch(
            state,
            Action::OpenEditor {
                id: if seed_id.is_empty() {
                    None
                } else {
                    Some(seed_id.clone())
                },
            },
        );
    });

    let draft = state.read().draft.clone();
    let show_advanced = state.read().editor_show_advanced;
    let group_choices = state.read().group_choices.clone();
    let group_discovery_loading = state.read().group_discovery_loading;
    let group_discovery_error = state.read().group_discovery_error.clone();
    let group_options: Vec<String> = group_choices
        .iter()
        .map(|choice| choice.label.clone())
        .collect();
    let selected_group = group_choices
        .iter()
        .position(|choice| {
            choice.name == draft.group.trim() || choice.label.trim() == draft.group.trim()
        })
        .and_then(|index| group_options.get(index).cloned())
        .or_else(|| group_options.first().cloned())
        .unwrap_or_default();

    let protocol_options: Vec<String> = ProtocolKind::all()
        .iter()
        .map(|p| p.as_label().to_owned())
        .collect();
    let selected_protocol = draft.protocol.as_label().to_owned();
    let split_auto = SplitTunnelMode::Auto.as_label().to_owned();
    let split_vpn = SplitTunnelMode::OnVpnDns.as_label().to_owned();
    let split_uplink = SplitTunnelMode::OnUplinkDns.as_label().to_owned();
    let selected_split = draft.split_tunnel_mode.as_label().to_owned();
    let token_disabled = SoftwareToken::Disabled.as_label().to_owned();
    let token_securid = SoftwareToken::SecurId.as_label().to_owned();
    let token_totp = SoftwareToken::Totp.as_label().to_owned();
    let selected_token = draft.software_token.as_label().to_owned();

    let auth_password = s.auth_password.to_owned();
    let auth_certificate = s.auth_certificate.to_owned();
    let auth_password_cert = s.auth_password_cert.to_owned();
    let auth_saml = s.auth_saml.to_owned();
    let selected_auth = match draft.auth_method {
        AuthMethod::Password => auth_password.clone(),
        AuthMethod::Certificate => auth_certificate.clone(),
        AuthMethod::PasswordAndCertificate => auth_password_cert.clone(),
        AuthMethod::Saml => auth_saml.clone(),
    };
    let auth_password_opt = auth_password.clone();
    let auth_certificate_opt = auth_certificate.clone();
    let auth_password_cert_opt = auth_password_cert.clone();
    let auth_options = vec![
        auth_password.clone(),
        auth_certificate.clone(),
        auth_password_cert.clone(),
        auth_saml.clone(),
    ];

    let mtu_value = if draft.mtu == 0 {
        String::new()
    } else {
        draft.mtu.to_string()
    };
    let show_password = matches!(
        draft.auth_method,
        AuthMethod::Password | AuthMethod::PasswordAndCertificate
    );
    let show_certificate = matches!(
        draft.auth_method,
        AuthMethod::Certificate | AuthMethod::PasswordAndCertificate
    );
    let show_username = !matches!(draft.auth_method, AuthMethod::Saml);

    let body = rsx! {
        column {
            width: "100%",
            align_items: "stretch",
            {section_label(s.basic)}
            {card(
                if id.is_empty() { s.add_connection } else { s.edit_connection },
                None,
                rsx! {
                    Form {
                        surface: false,
                        submit_label: String::new(),
                        FormItem {
                            label: s.name.to_owned(),
                            Input {
                                value: Some(draft.name.clone()),
                                width: Some("100%".to_owned()),
                                on_change: move |value| dispatch(state, Action::SetDraftName(value)),
                            }
                        }
                        FormItem {
                            label: s.server.to_owned(),
                            Input {
                                value: Some(draft.server.clone()),
                                width: Some("100%".to_owned()),
                                placeholder: Some("vpn.example.com".to_owned()),
                                on_change: move |value| dispatch(state, Action::SetDraftServer(value)),
                            }
                        }
                        FormItem {
                            label: s.group.to_owned(),
                            column {
                                width: "100%",
                                align_items: "stretch",
                                if !group_choices.is_empty() {
                                    {
                                        let choices_for_handler = group_choices.clone();
                                        let options = group_options.clone();
                                        let current = selected_group.clone();
                                        let default_current = current.clone();
                                        rsx! {
                                            Select {
                                                options,
                                                selected: Some(current),
                                                default_selected: default_current,
                                                open: None,
                                                default_open: false,
                                                on_open_change: None,
                                                on_select: Some(EventHandler::new(move |label: String| {
                                                    if let Some(choice) = choices_for_handler
                                                        .iter()
                                                        .find(|choice| choice.label == label)
                                                    {
                                                        dispatch(
                                                            state,
                                                            Action::SetDraftGroup(choice.name.clone()),
                                                        );
                                                    }
                                                })),
                                            }
                                        }
                                    }
                                } else {
                                    Input {
                                        value: Some(draft.group.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some(if group_discovery_loading {
                                            tr(
                                                current.locale,
                                                "正在获取服务器分组…",
                                                "Fetching server groups…",
                                            ).to_owned()
                                        } else {
                                            "Employees".to_owned()
                                        }),
                                        disabled: group_discovery_loading,
                                        on_change: move |value| dispatch(state, Action::SetDraftGroup(value)),
                                    }
                                }
                                if group_discovery_loading {
                                    row {
                                        margin_top: 6.0,
                                        align_items: "center",
                                        Spinner { size: 14.0, color: Some(subtle()) }
                                        text {
                                            content: tr(
                                                current.locale,
                                                "正在读取 AnyConnect 认证分组",
                                                "Reading AnyConnect authentication groups",
                                            ),
                                            margin_left: 6.0,
                                            font_size: 12.0,
                                            font_color: subtle(),
                                        }
                                    }
                                } else if let Some(error) = group_discovery_error.clone() {
                                    text {
                                        content: error,
                                        margin_top: 6.0,
                                        font_size: 11.0,
                                        max_lines: 2_i32,
                                        text_overflow: "ellipsis",
                                        font_color: subtle(),
                                    }
                                }
                            }
                        }
                        FormItem {
                            label: s.auth_method.to_owned(),
                            Select {
                                options: auth_options.clone(),
                                selected: Some(selected_auth.clone()),
                                default_selected: selected_auth.clone(),
                                open: None,
                                default_open: false,
                                on_open_change: None,
                                on_select: Some(EventHandler::new(move |value: String| {
                                    let method = if value == auth_password_opt {
                                        AuthMethod::Password
                                    } else if value == auth_certificate_opt {
                                        AuthMethod::Certificate
                                    } else if value == auth_password_cert_opt {
                                        AuthMethod::PasswordAndCertificate
                                    } else {
                                        AuthMethod::Saml
                                    };
                                    dispatch(state, Action::SetDraftAuthMethod(method));
                                })),
                            }
                        }
                        if show_username {
                            FormItem {
                                label: s.username.to_owned(),
                                Input {
                                    value: Some(draft.username.clone()),
                                    width: Some("100%".to_owned()),
                                    on_change: move |value| dispatch(state, Action::SetDraftUsername(value)),
                                }
                            }
                        }
                        if show_password {
                            FormItem {
                                label: s.password.to_owned(),
                                Input {
                                    value: Some(draft.password.clone()),
                                    width: Some("100%".to_owned()),
                                    mode: InputMode::Password,
                                    on_change: move |value| dispatch(state, Action::SetDraftPassword(value)),
                                }
                            }
                        }
                        if show_certificate {
                            FormItem {
                                label: s.certificate.to_owned(),
                                column {
                                    width: "100%",
                                    align_items: "stretch",
                                    Input {
                                        value: Some(draft.certificate.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some(tr(
                                            current.locale,
                                            "客户端证书路径 PEM/P12",
                                            "Client certificate path PEM/P12",
                                        ).to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftCertificate(value)),
                                    }
                                    row {
                                        width: "100%",
                                        margin_top: 8.0,
                                        justify_content: "end",
                                        FlatButton {
                                            variant: FlatButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            onclick: move |_| {
                                                dispatch(
                                                    state,
                                                    Action::PickCertFile(
                                                        crate::platform_callbacks::CertFileKind::Certificate,
                                                    ),
                                                );
                                            },
                                            {arkit::icon("folder-open", 14.0, accent())}
                                            text {
                                                content: tr(current.locale, "选择文件", "Browse").to_owned(),
                                                margin_left: 6.0,
                                                font_size: 13.0,
                                                font_color: accent(),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if matches!(draft.auth_method, AuthMethod::Saml) {
                            {switch_row(
                                s.external_browser,
                                tr(
                                    current.locale,
                                    "系统浏览器完成 SAML 登录",
                                    "SAML login in system browser",
                                ),
                                draft.external_browser_auth,
                                EventHandler::new(move |value| {
                                    dispatch(state, Action::SetDraftExternalBrowserAuth(value));
                                }),
                            )}
                        }
                        {switch_row(
                            s.favorite,
                            tr(current.locale, "在列表中优先展示", "Pin near the top of the list"),
                            draft.favorite,
                            EventHandler::new(move |value| {
                                dispatch(state, Action::SetDraftFavorite(value));
                            }),
                        )}
                        {switch_row(
                            s.force_global,
                            s.force_global_desc,
                            draft.force_global,
                            EventHandler::new(move |value| {
                                dispatch(state, Action::SetDraftForceGlobal(value));
                            }),
                        )}
                    }
                }
            )}
            row { height: 14.0 }
            {card(
                tr(current.locale, "高级配置", "Advanced").to_owned(),
                Some(tr(
                    current.locale,
                    "关闭时使用默认值，不展示非常用选项",
                    "When off, uncommon options stay hidden and use defaults",
                ).to_owned()),
                rsx! {
                    column {
                        width: "100%",
                        align_items: "stretch",
                        {switch_row(
                            tr(current.locale, "显示高级选项", "Show advanced options"),
                            tr(
                                current.locale,
                                "协议、证书细节、代理、分流与令牌等",
                                "Protocol, cert details, proxy, split tunnel, tokens…",
                            ),
                            show_advanced,
                            EventHandler::new(move |value| {
                                dispatch(state, Action::SetEditorShowAdvanced(value));
                            }),
                        )}
                        if show_advanced {
                            row { height: 12.0 }
                            Form {
                                surface: false,
                                submit_label: String::new(),
                                FormItem {
                                    label: s.protocol.to_owned(),
                                    Select {
                                        options: protocol_options.clone(),
                                        selected: Some(selected_protocol.clone()),
                                        default_selected: selected_protocol.clone(),
                                        open: None,
                                        default_open: false,
                                        on_open_change: None,
                                        on_select: Some(EventHandler::new(move |value: String| {
                                            dispatch(state, Action::SetDraftProtocol(ProtocolKind::from_label(&value)));
                                        })),
                                    }
                                }
                                if show_certificate {
                                    FormItem {
                                        label: tr(current.locale, "私钥路径（可选）", "Private key path (optional)").to_owned(),
                                        column {
                                            width: "100%",
                                            align_items: "stretch",
                                            Input {
                                                value: Some(draft.private_key.clone()),
                                                width: Some("100%".to_owned()),
                                                placeholder: Some("key.pem".to_owned()),
                                                on_change: move |value| dispatch(state, Action::SetDraftPrivateKey(value)),
                                            }
                                            row {
                                                width: "100%",
                                                margin_top: 8.0,
                                                justify_content: "end",
                                                FlatButton {
                                                    variant: FlatButtonVariant::Outline,
                                                    size: ButtonSize::Sm,
                                                    onclick: move |_| {
                                                        dispatch(
                                                            state,
                                                            Action::PickCertFile(
                                                                crate::platform_callbacks::CertFileKind::PrivateKey,
                                                            ),
                                                        );
                                                    },
                                                    {arkit::icon("folder-open", 14.0, accent())}
                                                    text {
                                                        content: tr(current.locale, "选择文件", "Browse").to_owned(),
                                                        margin_left: 6.0,
                                                        font_size: 13.0,
                                                        font_color: accent(),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    FormItem {
                                        label: tr(current.locale, "证书口令 (PKCS#12/PEM)", "Key password (PKCS#12/PEM)").to_owned(),
                                        Input {
                                            value: Some(draft.key_password.clone()),
                                            width: Some("100%".to_owned()),
                                            mode: InputMode::Password,
                                            on_change: move |value| dispatch(state, Action::SetDraftKeyPassword(value)),
                                        }
                                    }
                                    FormItem {
                                        label: tr(
                                            current.locale,
                                            "第二客户端证书（MCA，可选）",
                                            "Secondary client certificate (MCA, optional)",
                                        ).to_owned(),
                                        Input {
                                            value: Some(draft.secondary_certificate.clone()),
                                            width: Some("100%".to_owned()),
                                            placeholder: Some("user-cert.pem / user.p12".to_owned()),
                                            on_change: move |value| dispatch(state, Action::SetDraftSecondaryCertificate(value)),
                                        }
                                    }
                                    FormItem {
                                        label: tr(
                                            current.locale,
                                            "第二证书私钥（可选）",
                                            "Secondary private key (optional)",
                                        ).to_owned(),
                                        Input {
                                            value: Some(draft.secondary_private_key.clone()),
                                            width: Some("100%".to_owned()),
                                            placeholder: Some("user-key.pem".to_owned()),
                                            on_change: move |value| dispatch(state, Action::SetDraftSecondaryPrivateKey(value)),
                                        }
                                    }
                                    FormItem {
                                        label: tr(
                                            current.locale,
                                            "第二证书口令",
                                            "Secondary key password",
                                        ).to_owned(),
                                        Input {
                                            value: Some(draft.secondary_key_password.clone()),
                                            width: Some("100%".to_owned()),
                                            mode: InputMode::Password,
                                            on_change: move |value| dispatch(state, Action::SetDraftSecondaryKeyPassword(value)),
                                        }
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "软件令牌", "Software token").to_owned(),
                                    Select {
                                        options: vec![token_disabled.clone(), token_securid.clone(), token_totp.clone()],
                                        selected: Some(selected_token.clone()),
                                        default_selected: selected_token.clone(),
                                        open: None,
                                        default_open: false,
                                        on_open_change: None,
                                        on_select: Some(EventHandler::new(move |value: String| {
                                            dispatch(state, Action::SetDraftSoftwareToken(SoftwareToken::from_label(&value)));
                                        })),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "令牌字符串", "Token string").to_owned(),
                                    Input {
                                        value: Some(draft.token_string.clone()),
                                        width: Some("100%".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftTokenString(value)),
                                    }
                                }
                                FormItem {
                                    label: s.backup_servers.to_owned(),
                                    Textarea {
                                        value: Some(draft.backup_servers.clone()),
                                        height: Some(56.0),
                                        width: Some("100%".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftBackupServers(value)),
                                    }
                                }
                                FormItem {
                                    label: format!("{} ({})", s.mtu_override, s.mtu_auto),
                                    Input {
                                        value: Some(mtu_value.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some("1400".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftMtu(value)),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "CA 证书路径", "CA certificate path").to_owned(),
                                    column {
                                        width: "100%",
                                        align_items: "stretch",
                                        Input {
                                            value: Some(draft.ca_certificate.clone()),
                                            width: Some("100%".to_owned()),
                                            on_change: move |value| dispatch(state, Action::SetDraftCaCertificate(value)),
                                        }
                                        row {
                                            width: "100%",
                                            margin_top: 8.0,
                                            justify_content: "end",
                                            FlatButton {
                                                variant: FlatButtonVariant::Outline,
                                                size: ButtonSize::Sm,
                                                onclick: move |_| {
                                                    dispatch(
                                                        state,
                                                        Action::PickCertFile(
                                                            crate::platform_callbacks::CertFileKind::CaCertificate,
                                                        ),
                                                    );
                                                },
                                                {arkit::icon("folder-open", 14.0, accent())}
                                                text {
                                                    content: tr(current.locale, "选择文件", "Browse").to_owned(),
                                                    margin_left: 6.0,
                                                    font_size: 13.0,
                                                    font_color: accent(),
                                                }
                                            }
                                        }
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "分流模式", "Split tunnel mode").to_owned(),
                                    Select {
                                        options: vec![split_auto.clone(), split_vpn.clone(), split_uplink.clone()],
                                        selected: Some(selected_split.clone()),
                                        default_selected: selected_split.clone(),
                                        open: None,
                                        default_open: false,
                                        on_open_change: None,
                                        on_select: Some(EventHandler::new(move |value: String| {
                                            dispatch(state, Action::SetDraftSplitTunnelMode(SplitTunnelMode::from_label(&value)));
                                        })),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "自定义分流网段", "Custom split networks").to_owned(),
                                    Textarea {
                                        value: Some(draft.split_tunnel_networks.clone()),
                                        height: Some(56.0),
                                        width: Some("100%".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftSplitTunnelNetworks(value)),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "上报 OS", "Reported OS").to_owned(),
                                    Input {
                                        value: Some(draft.reported_os.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some("OpenHarmony".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftReportedOs(value)),
                                    }
                                }
                                FormItem {
                                    label: "SNI".to_owned(),
                                    Input {
                                        value: Some(draft.sni.clone()),
                                        width: Some("100%".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftSni(value)),
                                    }
                                }
                                FormItem {
                                    label: "User-Agent".to_owned(),
                                    Input {
                                        value: Some(draft.user_agent.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some(hanyconnect_core::default_user_agent()),
                                        on_change: move |value| dispatch(state, Action::SetDraftUserAgent(value)),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "客户端版本", "Client version").to_owned(),
                                    Input {
                                        value: Some(draft.client_version.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some(hanyconnect_core::default_client_version()),
                                        on_change: move |value| dispatch(state, Action::SetDraftClientVersion(value)),
                                    }
                                }
                                FormItem {
                                    label: "DPD (s)".to_owned(),
                                    Input {
                                        value: Some(if draft.dpd_seconds == 0 { String::new() } else { draft.dpd_seconds.to_string() }),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some("0".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftDpdSeconds(value)),
                                    }
                                }
                                FormItem {
                                    label: "CSD wrapper".to_owned(),
                                    Input {
                                        value: Some(draft.csd_wrapper.clone()),
                                        width: Some("100%".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftCsdWrapper(value)),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "HTTP 代理", "HTTP proxy").to_owned(),
                                    Input {
                                        value: Some(draft.http_proxy.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some("http://proxy.example.com:8080".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftHttpProxy(value)),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "服务器证书钉扎", "Server cert pin").to_owned(),
                                    Input {
                                        value: Some(draft.server_cert_hash.clone()),
                                        width: Some("100%".to_owned()),
                                        placeholder: Some("pin-sha256:… / sha256:…".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftServerCertHash(value)),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "信任应用包名", "Trusted app packages").to_owned(),
                                    Textarea {
                                        value: Some(draft.trusted_applications.clone()),
                                        height: Some(48.0),
                                        width: Some("100%".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftTrustedApplications(value)),
                                    }
                                }
                                FormItem {
                                    label: tr(current.locale, "排除应用包名", "Blocked app packages").to_owned(),
                                    Textarea {
                                        value: Some(draft.blocked_applications.clone()),
                                        height: Some(48.0),
                                        width: Some("100%".to_owned()),
                                        on_change: move |value| dispatch(state, Action::SetDraftBlockedApplications(value)),
                                    }
                                }
                                {switch_row(
                                    "DTLS",
                                    tr(current.locale, "启用 DTLS 数据通道（推荐）", "Enable DTLS data path (recommended)"),
                                    draft.use_dtls,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftUseDtls(value));
                                    }),
                                )}
                                {switch_row(
                                    "PFS",
                                    tr(current.locale, "要求完美前向保密", "Require perfect forward secrecy"),
                                    draft.require_pfs,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftRequirePfs(value));
                                    }),
                                )}
                                {switch_row(
                                    "XML POST",
                                    tr(current.locale, "禁用 XML POST（少数网关需要）", "Disable XML POST (rare gateways)"),
                                    draft.disable_xml_post,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftDisableXmlPost(value));
                                    }),
                                )}
                                {switch_row(
                                    s.strict_cert,
                                    tr(current.locale, "拒绝主机名不匹配或不完整证书链", "Reject hostname mismatch or incomplete chains"),
                                    draft.strict_certificate_trust,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftStrictCertificateTrust(value));
                                    }),
                                )}
                                {switch_row(
                                    s.block_untrusted,
                                    tr(current.locale, "服务器不受信任时中止连接", "Abort when the server is untrusted"),
                                    draft.block_untrusted_servers,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftBlockUntrustedServers(value));
                                    }),
                                )}
                                {switch_row(
                                    tr(current.locale, "允许不安全加密", "Allow insecure cryptography"),
                                    tr(
                                        current.locale,
                                        "仅用于必须使用 3DES/RC4/SHA1 的旧网关；与证书信任无关",
                                        "Only for legacy 3DES/RC4/SHA1 gateways; independent of certificate trust",
                                    ),
                                    draft.allow_insecure_crypto,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftAllowInsecureCrypto(value));
                                    }),
                                )}
                                {switch_row(
                                    s.local_lan,
                                    tr(current.locale, "VPN 连接期间仍可访问本地网络", "Keep local network reachable while VPN is up"),
                                    draft.allow_local_lan,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftAllowLocalLan(value));
                                    }),
                                )}
                                {switch_row(
                                    s.connect_on_demand,
                                    tr(current.locale, "网络可用时自动建立隧道", "Bring the tunnel up when the network is available"),
                                    draft.connect_on_demand,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftConnectOnDemand(value));
                                    }),
                                )}
                                {switch_row(
                                    s.fips_mode,
                                    tr(
                                        current.locale,
                                        "当前运行时不提供已认证的 FIPS Provider",
                                        "The current runtime has no validated FIPS provider",
                                    ),
                                    draft.fips_mode,
                                    EventHandler::new(move |value| {
                                        dispatch(state, Action::SetDraftFipsMode(value));
                                    }),
                                )}
                            }
                        }
                    }
                }
            )}
            row { height: 8.0 }
        }
    };

    let actions = rsx! {
        FlatButton {
            variant: FlatButtonVariant::Ghost,
            size: ButtonSize::Icon,
            onclick: move |_| {
                let ok = {
                    let draft = state.read().draft.clone();
                    !draft.name.trim().is_empty() && !draft.server.trim().is_empty()
                };
                dispatch(state, Action::SaveDraft);
                if ok {
                    navigator.replace(Route::Connections {});
                }
            },
            {arkit::icon("save", 20.0, accent())}
        }
    };

    scaffold(state, Route::ConnectionEditor { id }, actions, body)
}
