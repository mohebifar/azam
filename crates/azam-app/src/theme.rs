//! The window's stylesheet: Blender's default dark theme.
//!
//! Colours follow Blender 4's "Blender Dark" preset: the window background shows as thin gaps
//! between rounded editor areas, headers sit on top of each area, and the accent is Blender's
//! selection blue.

/// The stylesheet for the whole window.
pub const CSS: &str = r"
html, body { margin: 0; height: 100%; }
body { background: #161616; color: #e6e6e6; font-size: 12px; overflow: hidden; }

.root { display: flex; flex-direction: column; height: 100vh; background: #161616; }

/* -- top bar ---------------------------------------------------------------------------- */
.topbar { display: flex; align-items: center; height: 28px; padding: 0 6px; gap: 2px;
          background: #181818; flex-shrink: 0; }
.logo { width: 28px; height: 22px; display: flex; align-items: center; justify-content: center; }
.menu-title { padding: 3px 8px; border-radius: 4px; color: #e6e6e6; cursor: default; white-space: nowrap; flex-shrink: 0; }
.menu-title:hover, .menu-title.open { background: #4772b3; color: #ffffff; }
.workspaces { display: flex; gap: 1px; margin-left: 14px; }
.ws-tab { padding: 4px 11px; border-radius: 5px 5px 0 0; color: #b3b3b3; background: transparent; }
.ws-tab:hover { color: #ffffff; background: #262626; }
.ws-tab.active { color: #ffffff; background: #303030; }
.topbar-spacer { flex: 1; }
.topbar-info { color: #8a8a8a; padding-right: 8px; }

/* -- dropdown menus ------------------------------------------------------------------------ */
.menu-scrim { position: absolute; left: 0; top: 0; right: 0; bottom: 0; }
.menu { position: absolute; min-width: 230px; padding: 4px 0;
        background: #181818; border: 1px solid #2b2b2b; border-radius: 6px;
        box-shadow: 0 4px 14px rgba(0,0,0,0.55); z-index: 20; }
.menu-item { display: flex; align-items: center; gap: 8px; height: 22px; padding: 0 10px; margin: 0 3px;
             border-radius: 3px; color: #e6e6e6; }
.menu-item:hover, .menu-item.open { background: #4772b3; color: #ffffff; }
.menu-item.disabled { color: #6b6b6b; }
.menu-item.disabled:hover { background: transparent; color: #6b6b6b; }
.menu-icon { width: 16px; height: 16px; flex-shrink: 0; }
.menu-label { flex: 1; }
.menu-path { color: #8f8f8f; white-space: nowrap; }
.menu-shortcut { color: #8f8f8f; font-size: 11px; }
.menu-item:hover .menu-shortcut { color: #dce4f2; }
.menu-sep { height: 1px; margin: 4px 8px; background: #2e2e2e; }

/* -- editor areas ---------------------------------------------------------------------------- */
.areas { flex: 1; display: flex; padding: 2px; min-height: 0; }
.area { display: flex; flex-direction: column; border-radius: 6px; overflow: hidden; background: #303030;
        min-width: 0; min-height: 0; }
.area-split-row { display: flex; flex-direction: row; min-width: 0; min-height: 0; overflow: hidden; }
.area-split-col { display: flex; flex-direction: column; min-width: 0; min-height: 0; overflow: hidden; }
.area-split { flex-shrink: 0; }
.area-split.v { width: 3px; cursor: ew-resize; }
.area-split.h { height: 3px; cursor: ns-resize; }
.area-split:hover { background: rgba(71,114,179,0.6); }
.area-header { display: flex; align-items: center; gap: 4px; height: 26px; padding: 0 5px; flex-shrink: 0;
               background: #303030; }
.area-body { flex: 1; position: relative; min-height: 0; display: flex; }
.editor-type { display: flex; align-items: center; gap: 2px; padding: 2px 4px; border-radius: 4px;
               background: #3d3d3d; }
.header-menu { padding: 3px 7px; border-radius: 4px; color: #e6e6e6; white-space: nowrap; flex-shrink: 0; }
.header-menu:hover { background: #474747; }
.header-sep { width: 8px; }
.header-spacer { flex: 1; }
.hbtn { display: flex; align-items: center; justify-content: center; height: 20px; min-width: 22px;
        padding: 0 3px; border-radius: 4px; background: #3d3d3d; color: #e6e6e6; }
.hbtn:hover { background: #4a4a4a; }
.hbtn.on { background: #4772b3; color: #ffffff; }
.hgroup { display: flex; gap: 1px; }
.tool-settings { gap: 4px; margin-left: 8px; }
.tool-header { background: #2b2b2b; border-top: 1px solid #1f1f1f; }
.hgroup .hbtn { border-radius: 0; }
.hgroup .hbtn.first { border-radius: 4px 0 0 4px; }
.hgroup .hbtn.last { border-radius: 0 4px 4px 0; }
.mode-label { padding: 0 6px; color: #e6e6e6; white-space: nowrap; flex-shrink: 0; }

/* -- tool shelf (T) --------------------------------------------------------------------------- */
.toolbar { position: absolute; left: 6px; top: 6px; display: flex; flex-direction: column; gap: 2px;
           padding: 3px; border-radius: 6px; background: rgba(40,40,40,0.92); z-index: 5;
           max-height: 94%; overflow-y: scroll; }
.toolbar .tool, .toolbar .tool-sep { flex-shrink: 0; }
.tool { width: 30px; height: 30px; display: flex; align-items: center; justify-content: center;
        border-radius: 4px; }
.tool:hover { background: #454545; }
.tool.active { background: #4772b3; }
.tool-sep { height: 1px; margin: 3px 2px; background: #474747; }
.toolbar.labelled .tool { width: 150px; justify-content: flex-start; gap: 6px; padding: 0 6px; }
.tool-label { white-space: nowrap; color: #e0e0e0; }
.tool.active .tool-label { color: #ffffff; }

/* -- viewport overlays ----------------------------------------------------------------------------- */
.vp-text { position: absolute; left: 50px; top: 8px; color: #e6e6e6; font-size: 12px; z-index: 4; }
.vp-sub { color: #b3b3b3; font-size: 11px; }
.nav-gizmo { position: absolute; right: 8px; top: 8px; z-index: 4; }
.vp-badge { position: absolute; right: 10px; bottom: 10px; padding: 3px 8px; border-radius: 4px;
            background: rgba(30,30,30,0.85); color: #d0d0d0; font-size: 11px; z-index: 4; }
.vp-badge.live { color: #9fe07a; }
.redo-panel { position: absolute; left: 52px; bottom: 40px; width: 210px; border-radius: 5px;
              background: rgba(56,56,56,0.97); border: 1px solid #222222; z-index: 6; overflow: hidden; }
.sidebar { position: absolute; right: 4px; top: 4px; width: 230px; display: flex; flex-direction: column;
           gap: 4px; z-index: 6; padding: 4px; border-radius: 6px; background: rgba(40,40,40,0.95); }
.arr-point { position: absolute; z-index: 3; width: 12px; height: 12px; border-radius: 6px; background: #4772b3;
             border: 2px solid #dfe8f7; box-shadow: 0 0 0 1px #111111; }
.arr-point:hover { background: #f1a33c; }
.fit-legend { position: absolute; left: 56px; bottom: 10px; padding: 6px 9px; border-radius: 4px;
              background: rgba(30,30,30,0.88); border: 1px solid #111111; display: flex; flex-direction: column;
              gap: 3px; font-size: 11px; color: #d0d0d0; z-index: 4; }
.fit-legend-title { color: #e6e6e6; font-weight: 600; }
.fit-legend-row { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
.fit-swatch { width: 12px; height: 12px; border-radius: 2px; border: 1px solid #111111; flex-shrink: 0; }

/* -- outliner ------------------------------------------------------------------------------------ */
.outliner { flex: 1; overflow-y: scroll; background: #282828; padding: 2px 0; }
.ol-row { display: flex; align-items: center; gap: 4px; height: 20px; padding-right: 6px; color: #e6e6e6; }
.ol-row:nth-child(odd) { background: #2b2b2b; }
.ol-row:hover { background: #363636; }
.ol-row.selected { background: #334d80; }
.ol-row.active { background: #4772b3; }
.ol-name { flex: 1; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.ol-dim { color: #8a8a8a; font-size: 11px; }
.ol-swatch { width: 10px; height: 10px; border-radius: 2px; flex-shrink: 0; }

/* -- properties ----------------------------------------------------------------------------------- */
.props { flex: 1; display: flex; min-height: 0; min-width: 0; background: #303030; }
.prop-tabs { width: 30px; display: flex; flex-direction: column; gap: 2px; padding: 4px 2px;
             background: #232323; flex-shrink: 0; }
.prop-tab { width: 26px; height: 26px; display: flex; align-items: center; justify-content: center;
            border-radius: 4px; }
.prop-tab:hover { background: #3a3a3a; }
.prop-tab.active { background: #4772b3; }
.prop-body { flex: 1; min-width: 0; overflow-y: scroll; padding: 6px 6px 20px 6px; display: flex; flex-direction: column;
             gap: 4px; }
.prop-title { display: flex; align-items: center; gap: 6px; padding: 2px 4px 6px 4px; color: #e6e6e6;
              font-weight: 600; }
.panel { border-radius: 5px; background: #383838; overflow: hidden; flex-shrink: 0; min-width: 0; }
.panel-head { display: flex; align-items: center; gap: 4px; height: 24px; padding: 0 6px;
              background: #3d3d3d; color: #e6e6e6; font-weight: 600; }
.panel-head:hover { background: #444444; }
.panel-body { display: flex; flex-direction: column; gap: 3px; padding: 6px 8px 8px 8px; min-width: 0; }
.prow { display: flex; align-items: center; gap: 6px; min-height: 20px; }
.plabel { width: 42%; text-align: right; color: #c8c8c8; flex-shrink: 0; overflow: hidden;
          white-space: nowrap; text-overflow: ellipsis; }
.pfield { flex: 1; min-width: 0; }
.pvalue { flex: 1; color: #e6e6e6; }
.pnote { color: #8f8f8f; font-size: 11px; padding: 2px 4px; white-space: nowrap; overflow: hidden;
         text-overflow: ellipsis; flex-shrink: 0; min-width: 0; }
.pbuttons { display: flex; gap: 2px; flex: 1; }
.pbtn { flex: 1; height: 20px; display: flex; align-items: center; justify-content: center; gap: 4px;
        padding: 0 8px; white-space: nowrap;
        border-radius: 4px; background: #545454; color: #e6e6e6; }
.pbtn:hover { background: #656565; }
.pbtn.primary { background: #4772b3; color: #ffffff; }
.pbtn.primary:hover { background: #5683c4; }
.pbtn.danger:hover { background: #a33a3a; }
.swatch-row { display: flex; flex-wrap: wrap; gap: 4px; }
.swatch { width: 20px; height: 20px; border-radius: 4px; border: 1px solid #1d1d1d; }
.swatch.selected { border: 2px solid #ffffff; }
.pom-table { overflow-x: auto; }
.pom-table table { width: 100%; border-collapse: collapse; font-size: 11px; }
.pom-table th { white-space: nowrap; text-align: left; color: #9d9d9d; font-weight: 600; padding: 2px 4px; border-bottom: 1px solid #474747; }
.pom-table td { padding: 2px 4px; border-bottom: 1px solid #333333; color: #e6e6e6; }
.techpack { flex: 1; overflow-y: scroll; padding: 12px 18px 30px 18px; background: #2b2b2b;
             display: flex; flex-direction: column; gap: 14px; }
.tp-section { background: #303030; border-radius: 6px; padding: 10px 12px; }
.tp-title { font-weight: 600; font-size: 13px; color: #e6e6e6; margin-bottom: 8px; }
.tp-table table { width: 100%; border-collapse: collapse; font-size: 12px; }
.tp-table th { text-align: left; color: #9d9d9d; font-weight: 600; padding: 4px 8px;
               border-bottom: 1px solid #474747; }
.tp-table td { padding: 4px 8px; border-bottom: 1px solid #383838; color: #e6e6e6; }
.tp-table tbody tr:hover { background: #363636; }
.tp-dot { width: 18px; height: 18px; border-radius: 9px; color: #111111; font-size: 10px; font-weight: 600;
          display: flex; align-items: center; justify-content: center; }
.tp-ok { color: #8fd16a; }
.tp-warn { color: #f0b04a; }
.reply { padding: 4px 6px; border-radius: 4px; background: #333333; display: flex; flex-direction: column; gap: 2px; }
.reply-head { color: #9d9d9d; font-size: 10.5px; }
.reply-text { color: #e6e6e6; }
.tooltip { position: absolute; z-index: 50; padding: 5px 8px; border-radius: 5px; background: #181818;
           border: 1px solid #333333; box-shadow: 0 3px 10px rgba(0,0,0,0.5); display: flex;
           flex-direction: column; gap: 2px; }
.tooltip-name { color: #e6e6e6; font-weight: 600; white-space: nowrap; }
.tooltip-key { color: #9d9d9d; font-size: 11px; white-space: nowrap; }
.pie-ring { position: absolute; width: 36px; height: 36px; border-radius: 18px; border: 3px solid #4772b3;
            box-shadow: 0 0 0 1px #111111; }
.empty { padding: 20px 10px; color: #8a8a8a; text-align: center; }
.hint { padding: 4px 6px; color: #8a8a8a; font-size: 11px; }

/* -- status bar ------------------------------------------------------------------------------------ */
.statusbar { display: flex; align-items: center; gap: 14px; height: 22px; padding: 0 10px;
             background: #181818; color: #b3b3b3; font-size: 11px; flex-shrink: 0; }
.status-hint { display: flex; align-items: center; gap: 4px; }
.status-spacer { flex: 1; }
.status-msg { color: #e6e6e6; }
.status-warn { color: #f0b04a; }

/* -- pie menu ------------------------------------------------------------------------------------- */
.pie { position: absolute; z-index: 30; }
.pie-item { position: absolute; display: flex; align-items: center; gap: 6px; padding: 4px 10px;
            border-radius: 12px; background: #2a2a2a; border: 1px solid #1a1a1a; color: #e6e6e6;
            white-space: nowrap; box-shadow: 0 2px 6px rgba(0,0,0,0.5); }
.pie-item:hover { background: #4772b3; color: #ffffff; }
.pie-title { position: absolute; color: #e6e6e6; font-weight: 600; white-space: nowrap; padding: 3px 10px;
             border-radius: 4px; background: #232323; border: 1px solid #111111;
             box-shadow: 0 2px 6px rgba(0,0,0,0.5); }

/* -- preferences ------------------------------------------------------------------------------ */
.prefs { position: absolute; left: 50%; top: 60px; width: 620px; margin-left: -310px; z-index: 45;
         background: #232323; border: 1px solid #111111; border-radius: 8px; padding: 8px;
         box-shadow: 0 8px 24px rgba(0,0,0,0.6); display: flex; flex-direction: column; max-height: 80%; }
.prefs-head { display: flex; align-items: center; gap: 6px; padding-bottom: 6px; border-bottom: 1px solid #2e2e2e; }
.prefs-title { font-weight: 600; color: #e6e6e6; margin-right: 10px; }
.prefs-body { display: flex; flex-direction: column; gap: 4px; padding-top: 6px; min-height: 0; }
.prefs-tab { padding: 3px 10px; border-radius: 4px; color: #bdbdbd; }
.prefs-tab:hover { background: #3a3a3a; }
.prefs-tab.on { background: #4772b3; color: #ffffff; }
.keymap-list { display: flex; flex-direction: column; max-height: 420px; overflow-y: scroll; margin-top: 4px; }
.keymap-row { display: flex; align-items: center; gap: 6px; padding: 2px 4px; border-radius: 4px; }
.keymap-row:hover { background: #2c2c2c; }
.keymap-name { flex: 1; color: #e6e6e6; white-space: nowrap; overflow: hidden; min-width: 0; }
.keymap-key { width: 120px; color: #8f8f8f; text-align: right; }
.keymap-key.custom { color: #e6e6e6; font-weight: 600; }

/* -- operator search (F3) ----------------------------------------------------------------------------- */
.search { position: absolute; left: 50%; top: 80px; width: 460px; margin-left: -230px; z-index: 40;
          background: #181818; border: 1px solid #2e2e2e; border-radius: 6px; padding: 6px;
          box-shadow: 0 6px 20px rgba(0,0,0,0.6); }
.search-results { display: flex; flex-direction: column; margin-top: 4px; max-height: 360px;
                  overflow-y: scroll; }
";
