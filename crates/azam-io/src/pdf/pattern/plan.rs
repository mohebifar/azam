//! Page planning of the pattern PDF: which pieces, how they are packed, and how many pages each
//! layout needs.

use azam_doc::{Piece, PieceState, Project, SizeName, seam_numbers};
use azam_geom::Vec2;
use serde::Serialize;

use super::{Layout, Paper, PatternPdfOptions};
use crate::dimensions::{DimTable, PieceGeom, dim_table, piece_geom};
use crate::format::{fixed, round_half_up};
use crate::model::{Bounds, pieces_in_id_order};

/// Room around a fully dimensioned piece for its dimension lines, in paper mm: left, top, right,
/// bottom.
pub const DIM_ROOM: [f64; 4] = [24.0, 23.0, 17.0, 17.0];
/// Below this drawing scale a piece gets overall dimensions only.
pub const DIM_FULL: f64 = 0.07;
/// Row height of the dimension tables, in mm.
pub const DIM_TABLE_ROW: f64 = 4.3;
/// Page margin of tiles and plotter sheets, in mm.
pub const SHEET_MARGIN: f64 = 10.0;
/// Gap between pieces on tiles, in mm.
const TILE_GAP: f64 = 15.0;
/// Gap between pieces on plotter sheets, in mm.
const PLOTTER_GAP: f64 = 20.0;
/// Longest plotter sheet, in mm (PDF viewers limit page size).
const MAX_SHEET_MM: f64 = 5000.0;
/// Room for the header and footer of a plotter sheet, in mm.
pub const PLOTTER_CHROME: f64 = 30.0;
/// Seams listed per sewing-schedule page.
pub const SEAMS_PER_PAGE: usize = 22;
/// Room around a piece drawn without dimensions on its own sheet, in mm.
const PIECE_ROOM: [f64; 4] = [8.0; 4];
/// Side margins of a piece sheet, in mm (both sides together).
const PIECE_SIDE_MARGINS: f64 = 24.0;
/// Header height of a piece sheet, in mm.
pub const PIECE_HEADER: f64 = 27.0;
/// Title-block height of a piece sheet, in mm.
const PIECE_FOOTER: f64 = 46.0;

/// One size of a piece drawn nested in the others.
#[derive(Clone, Debug)]
pub struct NestOutline {
    /// The size.
    pub size: SizeName,
    /// Index of the size in the size range.
    pub index: usize,
    /// Its sewing line.
    pub poly: Vec<Vec2>,
}

/// How a piece fits its own sheet.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PieceFit {
    /// Whether the sheet is landscape.
    pub landscape: bool,
    /// Sheet width, mm.
    pub page_w: f64,
    /// Sheet height, mm.
    pub page_h: f64,
    /// Drawing scale (at most 1).
    pub k: f64,
    /// Room around the piece: left, top, right, bottom.
    pub room: [f64; 4],
    /// Drawing area width, mm.
    pub area_w: f64,
    /// Drawing area height, mm.
    pub area_h: f64,
}

/// A piece to print.
#[derive(Clone, Debug)]
pub struct PdfItem<'a> {
    /// The piece.
    pub piece: &'a Piece,
    /// Its geometry in the active size.
    pub geom: PieceGeom,
    /// Every size, when sizes are nested.
    pub nest: Option<Vec<NestOutline>>,
    /// Bounds of everything drawn for it (cut line and nested sizes).
    pub bounds: Bounds,
    /// Its own sheet, in the piece-per-page layout.
    pub fit: Option<PieceFit>,
}

/// Position of one packed item, in mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Placement {
    /// x of the item's bounds.
    pub x: f64,
    /// y of the item's bounds.
    pub y: f64,
    /// Width including room.
    pub w: f64,
    /// Height including room.
    pub h: f64,
}

/// One shelf of a packing.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PackRow {
    /// Top of the shelf.
    pub y: f64,
    /// Height of the shelf.
    pub h: f64,
    /// Items on it, in placement order.
    pub idx: Vec<usize>,
}

/// Shelf packing of items into a width.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pack {
    /// Placement of each item (by item index).
    pub pos: Vec<Placement>,
    /// Width used.
    pub w: f64,
    /// Height used.
    pub h: f64,
    /// Shelves.
    pub rows: Vec<PackRow>,
    /// Room reserved around each item: left, top, right, bottom.
    pub room: [f64; 4],
}

/// Packs boxes tallest first into shelves of `width`, with `room` around each box.
#[must_use]
pub fn pack(boxes: &[Bounds], width: f64, gap: f64, room: [f64; 4]) -> Pack {
    let [l, t, r, b] = room;
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&i, &j| {
        boxes[j]
            .h
            .partial_cmp(&boxes[i].h)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let (mut x, mut y, mut row_h, mut max_w) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let mut pos = vec![
        Placement {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        };
        boxes.len()
    ];
    let mut rows = vec![PackRow {
        y: 0.0,
        h: 0.0,
        idx: Vec::new(),
    }];
    for i in order {
        let (w, h) = (boxes[i].w + l + r, boxes[i].h + t + b);
        if x > 0.0 && x + w > width {
            y += row_h + gap;
            x = 0.0;
            row_h = 0.0;
            rows.push(PackRow {
                y,
                h: 0.0,
                idx: Vec::new(),
            });
        }
        pos[i] = Placement {
            x: x + l,
            y: y + t,
            w,
            h,
        };
        x += w + gap;
        row_h = row_h.max(h);
        if let Some(row) = rows.last_mut() {
            row.idx.push(i);
            row.h = row_h;
        }
        max_w = max_w.max(x - gap);
    }
    Pack {
        pos,
        w: max_w,
        h: y + row_h,
        rows,
        room,
    }
}

/// How a piece fits a sheet of `paper` with its title block (and dimension room when `dims`).
#[must_use]
pub fn piece_fit(bounds: &Bounds, paper: Paper, dims: bool) -> PieceFit {
    let [short, long] = paper.size();
    let landscape = bounds.w > bounds.h;
    let (page_w, page_h) = if landscape { (long, short) } else { (short, long) };
    let room = if dims { DIM_ROOM } else { PIECE_ROOM };
    let area_w = page_w - PIECE_SIDE_MARGINS - room[0] - room[2];
    let area_h = page_h - PIECE_HEADER - PIECE_FOOTER - room[1] - room[3];
    let k = 1.0_f64.min(area_w / bounds.w).min(area_h / bounds.h);
    PieceFit {
        landscape,
        page_w,
        page_h,
        k,
        room,
        area_w,
        area_h,
    }
}

/// The pieces to print, in pattern order: the chosen ones, else every piece taking part in the
/// simulation.
#[must_use]
pub fn pdf_items<'a>(project: &'a Project, o: &PatternPdfOptions) -> Vec<PdfItem<'a>> {
    let mut pieces: Vec<&Piece> = pieces_in_id_order(project)
        .into_iter()
        .filter(|pc| pc.pts.len() > 2)
        .filter(|pc| {
            if o.piece_ids.is_empty() {
                pc.state != PieceState::Off
            } else {
                o.piece_ids.contains(&pc.id)
            }
        })
        .collect();
    pieces.sort_by_key(|pc| pc.order);
    let nested = o.all_sizes && project.sizes.len() > 1;
    pieces
        .into_iter()
        .map(|pc| {
            let geom = piece_geom(project, pc, None);
            let nest = nested.then(|| {
                project
                    .sizes
                    .iter()
                    .enumerate()
                    .map(|(index, size)| NestOutline {
                        size: size.clone(),
                        index,
                        poly: pc.graded_outline(size, &project.base_size).poly,
                    })
                    .collect::<Vec<_>>()
            });
            let bounds = match &nest {
                Some(nest) => Bounds::of(
                    geom.cut
                        .iter()
                        .copied()
                        .chain(nest.iter().flat_map(|n| n.poly.iter().copied())),
                ),
                None => geom.cb,
            };
            PdfItem {
                piece: pc,
                geom,
                nest,
                bounds,
                fit: None,
            }
        })
        .collect()
}

/// A block of the dimension tables: one piece's table at height `y` of its column.
#[derive(Clone, Debug)]
pub struct DimBlock {
    /// Index of the item.
    pub item: usize,
    /// The table.
    pub table: DimTable,
    /// Offset from the top of the column, in mm.
    pub y: f64,
}

/// The dimension tables flowed into two columns per landscape page.
#[must_use]
pub fn dim_table_pages(project: &Project, items: &[PdfItem], paper: Paper) -> Vec<[Vec<DimBlock>; 2]> {
    let nums = seam_numbers(project);
    let avail = paper.size()[0] - 32.0 - 16.0;
    let mut pages = Vec::new();
    let mut page: [Vec<DimBlock>; 2] = [Vec::new(), Vec::new()];
    let (mut col, mut y) = (0, 0.0);
    for (i, it) in items.iter().enumerate() {
        let table = dim_table(project, it.piece, &it.geom.outline, &nums);
        let notch_rows = if table.notches.is_empty() {
            0
        } else {
            table.notches.len() + 1
        };
        let need = (3 + table.model.corners.len() + notch_rows) as f64 * DIM_TABLE_ROW + 4.0;
        if y + need > avail && y > 0.0 {
            col += 1;
            y = 0.0;
            if col > 1 {
                pages.push(std::mem::take(&mut page));
                col = 0;
            }
        }
        page[col].push(DimBlock { item: i, table, y });
        y += need;
    }
    if !page[0].is_empty() {
        pages.push(page);
    }
    pages
}

/// Tiles of the full-scale tiled layout.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TiledPlan {
    /// Packing of the pieces at full scale.
    pub pack: Pack,
    /// Tile columns.
    pub cols: usize,
    /// Tile rows.
    pub rows: usize,
    /// Printable width of a tile, mm.
    pub tile_w: f64,
    /// Printable height of a tile, mm.
    pub tile_h: f64,
    /// Tiles with something on them, as `(row, column)`, row by row.
    pub used: Vec<(usize, usize)>,
}

/// One plotter sheet: a run of packing shelves.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sheet {
    /// Top of the run in the packing.
    pub y0: f64,
    /// Height of the run.
    pub h: f64,
    /// Items on the sheet.
    pub idx: Vec<usize>,
}

/// Sheets of the plotter layout.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlotterPlan {
    /// Packing of the pieces at full scale.
    pub pack: Pack,
    /// Sheet width (the roll, widened to the widest piece), mm.
    pub roll: f64,
    /// Sheets.
    pub sheets: Vec<Sheet>,
}

/// Layout-specific part of a plan.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanSheets {
    /// Nothing to print.
    Empty,
    /// One sheet per piece (fits are on the items).
    Pieces,
    /// All pieces on one sheet.
    Overview,
    /// Full scale on paper tiles.
    Tiled(TiledPlan),
    /// Full scale on plotter sheets.
    Plotter(PlotterPlan),
}

/// The page plan of a pattern PDF.
#[derive(Clone, Debug)]
pub struct PdfPlan<'a> {
    /// Pieces to print.
    pub items: Vec<PdfItem<'a>>,
    /// Total pages.
    pub pages: usize,
    /// Description of the output for the user.
    pub note: String,
    /// Pages of the sewing schedule.
    pub sew_pages: usize,
    /// Pages of dimension tables.
    pub dim_pages: usize,
    /// Layout details.
    pub sheets: PlanSheets,
}

/// Plans the pages of a pattern PDF.
#[must_use]
pub fn plan<'a>(project: &'a Project, o: &PatternPdfOptions) -> PdfPlan<'a> {
    let mut items = pdf_items(project, o);
    let paper = o.paper;
    let sew_pages = project.sewing.len().div_ceil(SEAMS_PER_PAGE);
    if items.is_empty() {
        return PdfPlan {
            items,
            pages: 0,
            note: "No pieces to export.".to_owned(),
            sew_pages,
            dim_pages: 0,
            sheets: PlanSheets::Empty,
        };
    }
    let dim_pages = if o.dims && o.layout != Layout::Plotter {
        dim_table_pages(project, &items, paper).len()
    } else {
        0
    };
    let tables = if o.dims { ", dimension tables" } else { "" };
    let room = if o.dims { DIM_ROOM } else { [0.0; 4] };
    let boxes: Vec<Bounds> = items.iter().map(|it| it.bounds).collect();
    let (pages, note, sheets) = match o.layout {
        Layout::Pieces => {
            let note = fit_pieces(&mut items, paper, o.dims);
            (items.len() + 1 + dim_pages + sew_pages, note, PlanSheets::Pieces)
        }
        Layout::Overview => (
            2 + dim_pages + sew_pages,
            format!(
                "All pieces on one {} sheet at a common scale, then a piece schedule{tables} and a sewing schedule.",
                paper.name()
            ),
            PlanSheets::Overview,
        ),
        Layout::Tiled => {
            let tiled = plan_tiles(&boxes, paper, room);
            let note = format!(
                "Full scale on {} {} tiles ({} × {} grid, blank tiles skipped) plus a cover with the assembly map and a test square{tables} and a sewing schedule. Print at 100 % / actual size.",
                tiled.used.len(),
                paper.name(),
                tiled.rows,
                tiled.cols
            );
            (
                tiled.used.len() + 1 + dim_pages + sew_pages,
                note,
                PlanSheets::Tiled(tiled),
            )
        }
        Layout::Plotter => {
            let plotter = plan_plotter(&boxes, o.plotter_width_mm, room);
            let note = plotter_note(&plotter);
            (plotter.sheets.len(), note, PlanSheets::Plotter(plotter))
        }
    };
    PdfPlan {
        items,
        pages,
        note,
        sew_pages,
        dim_pages,
        sheets,
    }
}

/// Fits each item to its own sheet; returns the description of the output.
fn fit_pieces(items: &mut [PdfItem], paper: Paper, dims: bool) -> String {
    let mut ones = 0;
    for it in items.iter_mut() {
        let f = piece_fit(&it.bounds, paper, dims);
        if f.k >= 0.999 {
            ones += 1;
        }
        it.fit = Some(f);
    }
    format!(
        "One CAD-style sheet per piece{}{}, with a title block and scale bar, then a piece schedule{} and a sewing schedule.",
        if dims {
            ", fully dimensioned (edge lengths, ordinates from a datum, angles, notch positions)"
        } else {
            ""
        },
        if ones > 0 {
            format!(" ({ones} at full scale)")
        } else {
            String::new()
        },
        if dims { ", dimension tables" } else { "" }
    )
}

/// Packs the pieces at full scale onto a grid of tiles and finds the tiles in use.
fn plan_tiles(boxes: &[Bounds], paper: Paper, room: [f64; 4]) -> TiledPlan {
    let [short, long] = paper.size();
    let (tw, th) = (short - 2.0 * SHEET_MARGIN, long - 2.0 * SHEET_MARGIN);
    let max_w = boxes
        .iter()
        .map(|b| b.w + room[0] + room[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let area: f64 = boxes
        .iter()
        .map(|b| (b.w + TILE_GAP + room[0] + room[2]) * (b.h + TILE_GAP + room[1] + room[3]))
        .sum();
    let cols = (max_w / tw).ceil().max(round_half_up(area.sqrt() / tw)).max(1.0) as usize;
    let pack = pack(boxes, cols as f64 * tw, TILE_GAP, room);
    let rows = (pack.h / th).ceil().max(1.0) as usize;
    let mut used = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let (x0, y0) = (c as f64 * tw, r as f64 * th);
            let hit = boxes.iter().zip(&pack.pos).any(|(b, p)| {
                p.x - room[0] < x0 + tw
                    && p.x + b.w + room[2] > x0
                    && p.y - room[1] < y0 + th
                    && p.y + b.h + room[3] > y0
            });
            if hit {
                used.push((r, c));
            }
        }
    }
    TiledPlan {
        pack,
        cols,
        rows,
        tile_w: tw,
        tile_h: th,
        used,
    }
}

/// Packs the pieces at full scale onto a roll and cuts it into sheets of at most 5 m.
fn plan_plotter(boxes: &[Bounds], roll: f64, room: [f64; 4]) -> PlotterPlan {
    let w = roll - 2.0 * SHEET_MARGIN;
    let max_w = boxes
        .iter()
        .map(|b| b.w + room[0] + room[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let pack = pack(boxes, w.max(max_w), PLOTTER_GAP, room);
    let max_h = MAX_SHEET_MM - 2.0 * SHEET_MARGIN - PLOTTER_CHROME;
    let mut sheets = Vec::new();
    let mut cur = Sheet {
        y0: 0.0,
        h: 0.0,
        idx: Vec::new(),
    };
    for row in &pack.rows {
        if !cur.idx.is_empty() && row.y + row.h - cur.y0 > max_h {
            let next = Sheet {
                y0: row.y,
                h: 0.0,
                idx: Vec::new(),
            };
            sheets.push(std::mem::replace(&mut cur, next));
        }
        cur.idx.extend_from_slice(&row.idx);
        cur.h = row.y + row.h - cur.y0;
    }
    sheets.push(cur);
    PlotterPlan {
        pack,
        roll: roll.max(max_w + 2.0 * SHEET_MARGIN),
        sheets,
    }
}

/// Description of the plotter output.
fn plotter_note(p: &PlotterPlan) -> String {
    let sheet_len = |s: &Sheet| s.h + 2.0 * SHEET_MARGIN + PLOTTER_CHROME;
    if let [only] = p.sheets.as_slice() {
        format!(
            "One full-scale sheet, {} × {} mm, for a plotter roll.",
            round_half_up(p.roll),
            round_half_up(sheet_len(only))
        )
    } else {
        let lens: Vec<String> = p
            .sheets
            .iter()
            .map(|s| format!("{} m", fixed(round_half_up(sheet_len(s) / 10.0) / 100.0, 2)))
            .collect();
        format!(
            "{} full-scale sheets {} mm wide ({}); PDF pages are limited to 5 m.",
            p.sheets.len(),
            round_half_up(p.roll),
            lens.join(" + ")
        )
    }
}
