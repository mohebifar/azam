//! Starting and stopping the Claude Code server from the app, and its panel in the 3D view's
//! sidebar (N): whether it runs, where, the command that connects Claude Code, and what the
//! agent did last.

use blenderui::prelude::*;

use super::http;
use crate::app::AzamApp;
use crate::operators::Operator;

impl AzamApp {
    /// Sets the port the Claude Code server listens on when started.
    pub fn set_mcp_port(&mut self, port: u16) {
        self.mcp_port = port;
    }

    /// Whether the Claude Code server is running.
    #[must_use]
    pub fn claude_server_running(&self) -> bool {
        http::is_running()
    }

    /// Starts the Claude Code server (the status bar says where, or why it could not).
    pub fn start_claude_server(&mut self, cx: &mut App) {
        self.status = match http::start(cx.ui_sender(), self.mcp_port) {
            Ok(()) => format!(
                "Claude Code server on {}: connect with `{}`",
                http::url(self.mcp_port),
                http::claude_command(self.mcp_port)
            ),
            Err(e) => format!("Could not start the Claude Code server: {e}"),
        };
    }

    /// Stops the Claude Code server.
    pub fn stop_claude_server(&mut self) {
        http::stop();
        self.status = "Claude Code server stopped".into();
    }

    /// Starts the Claude Code server, or stops it when it runs.
    pub fn toggle_claude_server(&mut self, cx: &mut App) {
        if http::is_running() {
            self.stop_claude_server();
        } else {
            self.start_claude_server(cx);
        }
    }

    /// The rows of the sidebar's Claude Code panel.
    pub(crate) fn claude_panel_rows(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let running = http::is_running();
        let button = div()
            .class(if running { "pbtn primary" } else { "pbtn" })
            .id("claude-server")
            .child(if running { "Stop Server" } else { "Start Server" })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.run(Operator::ClaudeServer, window, cx);
            }));
        let mut rows = vec![
            Self::info_row("Server", if running { "Running" } else { "Off" }.to_owned()),
            div().class("pbuttons").child(button),
        ];
        if !running {
            rows.push(
                div()
                    .class("hint")
                    .child("Start the server to let Claude Code design in this window."),
            );
            return rows;
        }
        let port = self.mcp_port;
        // the full URL is too wide for the sidebar; the copied command carries it
        rows.push(Self::info_row("Address", format!("127.0.0.1:{port}")));
        rows.push(
            div().class("pbuttons").child(
                div()
                    .class("pbtn")
                    .id("claude-copy")
                    .child("Copy Setup Command")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        window.write_clipboard(&http::claude_command(port));
                        this.status = "Copied: paste it in a terminal to connect Claude Code".into();
                        cx.notify();
                    })),
            ),
        );
        let (client, last) = http::activity();
        rows.push(Self::info_row(
            "Client",
            client.unwrap_or_else(|| "Not connected yet".to_owned()),
        ));
        if let Some((tool, _)) = last {
            rows.push(Self::info_row("Last", tool.replace('_', " ")));
        }
        rows
    }
}
