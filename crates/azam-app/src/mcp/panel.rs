//! Starting and stopping the MCP server from the app, and its panel in the 3D view's
//! sidebar (N): whether it runs, its address, which client is connected and what it did last.

use blenderui::prelude::*;

use super::http;
use crate::app::AzamApp;
use crate::operators::Operator;

impl AzamApp {
    /// Sets the port the MCP server listens on when started.
    pub fn set_mcp_port(&mut self, port: u16) {
        self.mcp_port = port;
    }

    /// Whether the MCP server is running.
    #[must_use]
    pub fn mcp_server_running(&self) -> bool {
        http::is_running()
    }

    /// Starts the MCP server (the status bar says where, or why it could not).
    pub fn start_mcp_server(&mut self, cx: &mut App) {
        self.status = match http::start(cx.ui_sender(), self.mcp_port) {
            Ok(()) => format!("MCP server running on {}", http::url(self.mcp_port)),
            Err(e) => format!("Could not start the MCP server: {e}"),
        };
    }

    /// Stops the MCP server.
    pub fn stop_mcp_server(&mut self) {
        http::stop();
        self.status = "MCP server stopped".into();
    }

    /// Starts the MCP server, or stops it when it runs.
    pub fn toggle_mcp_server(&mut self, cx: &mut App) {
        if http::is_running() {
            self.stop_mcp_server();
        } else {
            self.start_mcp_server(cx);
        }
    }

    /// The rows of the sidebar's MCP panel.
    pub(crate) fn mcp_panel_rows(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let running = http::is_running();
        let button = div()
            .class(if running { "pbtn primary" } else { "pbtn" })
            .id("mcp-server")
            .child(if running { "Stop Server" } else { "Start Server" })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.run(Operator::McpServer, window, cx);
            }));
        let mut rows = vec![
            Self::info_row("Server", if running { "Running" } else { "Off" }.to_owned()),
            div().class("pbuttons").child(button),
        ];
        if !running {
            rows.push(
                div()
                    .class("hint")
                    .child("Start the server so MCP clients can design in this window."),
            );
            return rows;
        }
        let port = self.mcp_port;
        // the full URL is too wide for the sidebar; Copy URL carries it
        rows.push(Self::info_row("Address", format!("127.0.0.1:{port}")));
        rows.push(
            div()
                .class("pbuttons")
                .child(
                    div()
                        .class("pbtn")
                        .id("mcp-copy")
                        .child("Copy URL")
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            window.write_clipboard(&http::url(port));
                            this.status = format!("Copied {}", http::url(port));
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
