use iced::widget::{
    PaneGrid, Space, button, column, container, mouse_area, pane_grid, row, scrollable, text,
    tooltip,
};
use iced::{Background, Border, Color, Element, Font, Length, Padding, Shadow, Task};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

mod loader;
mod theme;

fn get_hover_color(idx: usize) -> Color {
    theme::accent_hover(idx)
}

enum PaneState {
    Source,
    Ir,
}

struct CompilerExplorer {
    project: Option<loader::LoadedProject>,
    source_lines: Vec<String>,
    ir_variables: Vec<loader::UiVariable>,
    ir_values: Vec<loader::UiValue>,
    ir_instructions: Vec<loader::UiIrInstruction>,

    // Toggle state for IR metadata sections
    variables_expanded: bool,
    values_expanded: bool,

    active_lines: HashMap<usize, usize>,
    active_color_index: usize,
    hovered_source_line: Option<usize>,
    collapsed_modules: HashSet<usize>,
    panes: pane_grid::State<PaneState>,
}

#[derive(Debug, Clone)]
enum Message {
    ToggleModule(usize),
    SelectObject {
        module_name: String,
        object_name: String,
    },
    HoverSourceLine(Option<usize>),
    Resized(pane_grid::ResizeEvent),
    ToggleVariables,
    ToggleValues,
}

fn build_line_colors(instructions: &[loader::UiIrInstruction]) -> HashMap<usize, usize> {
    let mut map = HashMap::new();
    let mut offset = 0;
    for instr in instructions {
        if let Some(idx) = instr.source_line_index {
            map.entry(idx).or_insert_with(|| {
                let current = offset;
                offset += 1;
                current
            });
        }
    }
    map
}

impl CompilerExplorer {
    fn new(project_path: PathBuf) -> (Self, Task<Message>) {
        let (mut panes, source_pane) = pane_grid::State::new(PaneState::Source);
        let (_, split) = panes
            .split(pane_grid::Axis::Vertical, source_pane, PaneState::Ir)
            .unwrap();
        panes.resize(split, 0.65);
        let project = loader::load_project(&project_path).ok();

        let mut explorer = Self {
            project,
            source_lines: vec![],
            ir_variables: vec![],
            ir_values: vec![],
            ir_instructions: vec![],
            variables_expanded: false,
            values_expanded: false,
            active_lines: HashMap::new(),
            active_color_index: 0,
            hovered_source_line: None,
            collapsed_modules: HashSet::new(),
            panes,
        };

        if let Some(ref proj) = explorer.project {
            if let Some((_, details)) = proj.details.iter().next() {
                explorer.source_lines = details.source_lines.clone();
                explorer.ir_variables = details.ir_variables.clone();
                explorer.ir_values = details.ir_values.clone();
                explorer.ir_instructions = details.ir_instructions.clone();
                explorer.active_lines = build_line_colors(&details.ir_instructions);
                explorer.active_color_index = details.color_index;
            }
        }
        (explorer, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleModule(id) => {
                if self.collapsed_modules.contains(&id) {
                    self.collapsed_modules.remove(&id);
                } else {
                    self.collapsed_modules.insert(id);
                }
            }
            Message::SelectObject {
                module_name,
                object_name,
            } => {
                if let Some(ref proj) = self.project {
                    if let Some(details) = proj
                        .details
                        .get(&format!("{}::{}", module_name, object_name))
                    {
                        self.source_lines = details.source_lines.clone();
                        self.ir_variables = details.ir_variables.clone();
                        self.ir_values = details.ir_values.clone();
                        self.ir_instructions = details.ir_instructions.clone();
                        self.active_lines = build_line_colors(&details.ir_instructions);
                        self.active_color_index = details.color_index;
                    }
                }
            }
            Message::HoverSourceLine(line) => self.hovered_source_line = line,
            Message::Resized(e) => self.panes.resize(e.split, e.ratio),
            Message::ToggleVariables => self.variables_expanded = !self.variables_expanded,
            Message::ToggleValues => self.values_expanded = !self.values_expanded,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        // ===== Header Bar =====
        let header = container(
            row![
                text("Compiler Explorer")
                    .font(Font::MONOSPACE)
                    .size(18)
                    .style(|_| iced::widget::text::Style {
                        color: Some(theme::ctp::LAVENDER),
                    }),
                text(" — IR Visualization")
                    .size(14)
                    .style(|_| iced::widget::text::Style {
                        color: Some(theme::ctp::OVERLAY1),
                    }),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::new(12.0))
        .style(|_| theme::header_bar());

        // ===== Sidebar =====
        let sidebar_content = {
            let mut sidebar_col = column![
                text("Modules")
                    .size(20)
                    .font(Font {
                        weight: iced::font::Weight::Bold,
                        ..Font::MONOSPACE
                    })
                    .style(|_| iced::widget::text::Style {
                        color: Some(theme::ctp::TEXT),
                    })
            ]
            .spacing(12);

            if let Some(ref proj) = self.project {
                for module in &proj.modules {
                    let is_collapsed = self.collapsed_modules.contains(&module.id);

                    // Module header button
                    let mod_btn = button(
                        row![
                            text(if is_collapsed { "▶" } else { "▼" })
                                .size(12)
                                .style(|_| iced::widget::text::Style {
                                    color: Some(theme::ctp::OVERLAY1),
                                }),
                            text(&module.name)
                                .size(14)
                                .style(|_| iced::widget::text::Style {
                                    color: Some(theme::ctp::SUBTEXT1),
                                }),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .on_press(Message::ToggleModule(module.id))
                    .style(move |_, _| button::Style {
                        background: if is_collapsed {
                            None
                        } else {
                            Some(Background::Color(theme::ctp::SURFACE0))
                        },
                        text_color: theme::ctp::SUBTEXT1,
                        border: Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: 6.0.into(),
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    })
                    .padding(Padding::new(6.0))
                    .width(Length::Fill);

                    sidebar_col = sidebar_col.push(mod_btn);

                    if !is_collapsed {
                        let mut obj_col = column![].spacing(4).padding(Padding::new(20.0).left);

                        for obj in &module.objects {
                            let export_icon = if obj.is_exported { "●" } else { "○" };
                            let icon_color = if obj.is_exported {
                                theme::ctp::GREEN
                            } else {
                                theme::ctp::OVERLAY0
                            };

                            let label = row![
                                text(export_icon).size(10).style(move |_| {
                                    iced::widget::text::Style {
                                        color: Some(icon_color),
                                    }
                                }),
                                text(&obj.name)
                                    .size(13)
                                    .style(|_| iced::widget::text::Style {
                                        color: if obj.is_polymorphic {
                                            Some(theme::ctp::OVERLAY1)
                                        } else {
                                            Some(theme::ctp::SUBTEXT0)
                                        },
                                    }),
                            ]
                            .spacing(8)
                            .align_y(iced::Alignment::Center);

                            if obj.is_polymorphic {
                                obj_col = obj_col.push(
                                    container(label)
                                        .padding(Padding::new(4.0))
                                        .width(Length::Fill),
                                );
                            } else {
                                obj_col = obj_col.push(
                                    button(label)
                                        .on_press(Message::SelectObject {
                                            module_name: module.name.clone(),
                                            object_name: obj.name.clone(),
                                        })
                                        .style(|_, _| button::Style {
                                            background: None,
                                            text_color: theme::ctp::SUBTEXT0,
                                            border: Border {
                                                color: Color::TRANSPARENT,
                                                width: 0.0,
                                                radius: 4.0.into(),
                                            },
                                            shadow: Shadow::default(),
                                            snap: false,
                                        })
                                        .padding(Padding::new(4.0))
                                        .width(Length::Fill),
                                );
                            }
                        }
                        sidebar_col = sidebar_col.push(obj_col);
                    }
                }
            }
            sidebar_col
        };

        let sidebar = container(scrollable(sidebar_content))
            .width(Length::FillPortion(1))
            .padding(12)
            .style(|_| theme::sidebar_panel());

        // ===== Pane Grid =====
        let pane_grid = PaneGrid::new(&self.panes, |_, state, _| {
            pane_grid::Content::new(match state {
                PaneState::Source => {
                    let mut col = column![].spacing(0);

                    for (index, line) in self.source_lines.iter().enumerate() {
                        let color_offset = self.active_lines.get(&index);
                        let is_hovered = self.hovered_source_line == Some(index);

                        let bg_color = if is_hovered {
                            color_offset
                                .map(|&offset| {
                                    theme::accent_hover(self.active_color_index + offset)
                                })
                                .or(Some(theme::ctp::SURFACE0))
                        } else if let Some(&offset) = color_offset {
                            Some(theme::accent_medium(self.active_color_index + offset))
                        } else {
                            None
                        };

                        let dimmed = self.hovered_source_line.is_some()
                            && !is_hovered
                            && color_offset.is_some();

                        let line_bg = if dimmed {
                            color_offset
                                .map(|&offset| theme::accent_dim(self.active_color_index + offset))
                        } else {
                            bg_color
                        };

                        col = col.push(
                            mouse_area(
                                container(
                                    row![
                                        // Line number
                                        container(
                                            text(format!("{:3}", index + 1))
                                                .font(Font::MONOSPACE)
                                                .size(13)
                                                .style(|_| iced::widget::text::Style {
                                                    color: Some(theme::ctp::OVERLAY0),
                                                }),
                                        )
                                        .width(Length::Fixed(40.0))
                                        .padding(Padding::new(0.0).right(8.0))
                                        .align_x(iced::alignment::Horizontal::Right),
                                        // Separator
                                        container(
                                            Space::new()
                                                .width(Length::Fixed(1.0))
                                                .height(Length::Fill)
                                        )
                                        .style(|_| {
                                            container::Style {
                                                background: Some(Background::Color(
                                                    theme::ctp::SURFACE0,
                                                )),
                                                ..Default::default()
                                            }
                                        }),
                                        // Code
                                        text(line).font(Font::MONOSPACE).size(13).style(|_| {
                                            iced::widget::text::Style {
                                                color: Some(theme::ctp::TEXT),
                                            }
                                        }),
                                    ]
                                    .spacing(8)
                                    .align_y(iced::Alignment::Center),
                                )
                                .width(Length::Fill)
                                .padding(Padding::new(2.0).left(8.0).right(8.0))
                                .style(move |_| theme::code_line_bg(line_bg)),
                            )
                            .on_enter(Message::HoverSourceLine(Some(index))),
                        );
                    }

                    let interactive_col = mouse_area(col).on_exit(Message::HoverSourceLine(None));
                    container(scrollable(interactive_col))
                        .padding(12)
                        .style(|_| theme::base_panel())
                }

                PaneState::Ir => {
                    let mut col = column![].spacing(16);

                    // =========================
                    // 1. Expandable Variables Table
                    // =========================
                    let mut var_col = column![
                        button(
                            row![
                                text(if self.variables_expanded {
                                    "▼"
                                } else {
                                    "▶"
                                })
                                .size(12)
                                .style(|_| {
                                    iced::widget::text::Style {
                                        color: Some(theme::ctp::OVERLAY1),
                                    }
                                }),
                                text("Variables")
                                    .size(14)
                                    .font(Font {
                                        weight: iced::font::Weight::Bold,
                                        ..Font::MONOSPACE
                                    })
                                    .style(|_| iced::widget::text::Style {
                                        color: Some(theme::ctp::SUBTEXT1),
                                    }),
                            ]
                            .spacing(8)
                            .align_y(iced::Alignment::Center),
                        )
                        .on_press(Message::ToggleVariables)
                        .style(|_, _| button::Style {
                            background: None,
                            text_color: theme::ctp::SUBTEXT1,
                            border: Border::default(),
                            shadow: Shadow::default(),
                            snap: false,
                        })
                        .padding(4)
                    ]
                    .spacing(8);

                    if self.variables_expanded && !self.ir_variables.is_empty() {
                        let mut table = column![
                            container(
                                row![
                                    text("id")
                                        .width(Length::Fixed(50.0))
                                        .font(Font::MONOSPACE)
                                        .size(12)
                                        .style(|_| iced::widget::text::Style {
                                            color: Some(theme::ctp::OVERLAY1),
                                        }),
                                    text("identifier")
                                        .width(Length::Fixed(160.0))
                                        .font(Font::MONOSPACE)
                                        .size(12)
                                        .style(|_| iced::widget::text::Style {
                                            color: Some(theme::ctp::OVERLAY1),
                                        }),
                                    text("type")
                                        .width(Length::Fill)
                                        .font(Font::MONOSPACE)
                                        .size(12)
                                        .style(|_| iced::widget::text::Style {
                                            color: Some(theme::ctp::OVERLAY1),
                                        }),
                                ]
                                .spacing(12)
                            )
                            .padding(Padding::new(8.0))
                            .style(|_| theme::table_header()),
                        ]
                        .spacing(2);

                        for v in &self.ir_variables {
                            table = table.push(
                                container(
                                    row![
                                        text(format!("{}", v.id))
                                            .width(Length::Fixed(50.0))
                                            .font(Font::MONOSPACE)
                                            .size(13)
                                            .style(|_| iced::widget::text::Style {
                                                color: Some(theme::ctp::SKY),
                                            }),
                                        text(&v.identifier)
                                            .width(Length::Fixed(160.0))
                                            .font(Font::MONOSPACE)
                                            .size(13)
                                            .style(|_| iced::widget::text::Style {
                                                color: Some(theme::ctp::TEXT),
                                            }),
                                        text(&v.ty)
                                            .width(Length::Fill)
                                            .font(Font::MONOSPACE)
                                            .size(13)
                                            .style(|_| iced::widget::text::Style {
                                                color: Some(theme::ctp::SUBTEXT0),
                                            }),
                                    ]
                                    .spacing(12)
                                    .align_y(iced::Alignment::Center),
                                )
                                .padding(Padding::new(6.0).left(8.0).right(8.0))
                                .style(|_| container::Style {
                                    background: Some(Background::Color(theme::ctp::BASE)),
                                    border: Border {
                                        color: theme::ctp::SURFACE0,
                                        width: 1.0,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                }),
                            );
                        }
                        var_col = var_col.push(table);
                    }
                    col = col.push(var_col);

                    // =========================
                    // 2. Expandable Values Table
                    // =========================
                    let mut val_col = column![
                        button(
                            row![
                                text(if self.values_expanded { "▼" } else { "▶" })
                                    .size(12)
                                    .style(|_| iced::widget::text::Style {
                                        color: Some(theme::ctp::OVERLAY1),
                                    }),
                                text("Values")
                                    .size(14)
                                    .font(Font {
                                        weight: iced::font::Weight::Bold,
                                        ..Font::MONOSPACE
                                    })
                                    .style(|_| iced::widget::text::Style {
                                        color: Some(theme::ctp::SUBTEXT1),
                                    }),
                            ]
                            .spacing(8)
                            .align_y(iced::Alignment::Center),
                        )
                        .on_press(Message::ToggleValues)
                        .style(|_, _| button::Style {
                            background: None,
                            text_color: theme::ctp::SUBTEXT1,
                            border: Border::default(),
                            shadow: Shadow::default(),
                            snap: false,
                        })
                        .padding(4)
                    ]
                    .spacing(8);

                    if self.values_expanded && !self.ir_values.is_empty() {
                        let mut table = column![
                            container(
                                row![
                                    text("id")
                                        .width(Length::Fixed(50.0))
                                        .font(Font::MONOSPACE)
                                        .size(12)
                                        .style(|_| iced::widget::text::Style {
                                            color: Some(theme::ctp::OVERLAY1),
                                        }),
                                    text("type")
                                        .width(Length::Fill)
                                        .font(Font::MONOSPACE)
                                        .size(12)
                                        .style(|_| iced::widget::text::Style {
                                            color: Some(theme::ctp::OVERLAY1),
                                        }),
                                ]
                                .spacing(12)
                            )
                            .padding(Padding::new(8.0))
                            .style(|_| theme::table_header()),
                        ]
                        .spacing(2);

                        for v in &self.ir_values {
                            table = table.push(
                                container(
                                    row![
                                        text(format!("{}", v.id))
                                            .width(Length::Fixed(50.0))
                                            .font(Font::MONOSPACE)
                                            .size(13)
                                            .style(|_| iced::widget::text::Style {
                                                color: Some(theme::ctp::SKY),
                                            }),
                                        text(&v.ty)
                                            .width(Length::Fill)
                                            .font(Font::MONOSPACE)
                                            .size(13)
                                            .style(|_| iced::widget::text::Style {
                                                color: Some(theme::ctp::TEXT),
                                            }),
                                    ]
                                    .spacing(12)
                                    .align_y(iced::Alignment::Center),
                                )
                                .padding(Padding::new(6.0).left(8.0).right(8.0))
                                .style(|_| container::Style {
                                    background: Some(Background::Color(theme::ctp::BASE)),
                                    border: Border {
                                        color: theme::ctp::SURFACE0,
                                        width: 1.0,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                }),
                            );
                        }
                        val_col = val_col.push(table);
                    }
                    col = col.push(val_col);

                    // =========================
                    // 3. Block & Instruction Render
                    // =========================
                    let mut instr_col = column![].spacing(0);

                    for instr in &self.ir_instructions {
                        // 1. Setup Colors
                        let offset = instr
                            .source_line_index
                            .and_then(|idx| self.active_lines.get(&idx).copied())
                            .unwrap_or(0);
                        let color_idx = self.active_color_index + offset;
                        let mut source_bg = theme::accent_medium(color_idx);

                        // Dim inactive lines
                        if let Some(hover) = self.hovered_source_line {
                            if Some(hover) != instr.source_line_index {
                                source_bg = theme::accent_dim(color_idx);
                            } else {
                                source_bg = theme::accent_hover(color_idx);
                            }
                        }

                        // 2. Handle Block Indicator
                        let block_color = theme::BLOCK[instr.block_idx % theme::BLOCK.len()];

                        let block_indicator = container(
                            Space::new()
                                .width(Length::Fixed(5.0))
                                .height(Length::Fixed(22.0)),
                        )
                        .style(move |_| container::Style {
                            background: Some(Background::Color(block_color)),
                            border: Border {
                                color: block_color,
                                width: 0.0,
                                radius: 2.0.into(),
                            },
                            ..Default::default()
                        });

                        // 3. Text Styling based on Kind
                        let mut text_widget = text(&instr.text)
                            .font(Font::MONOSPACE)
                            .size(13)
                            .style(|_| iced::widget::text::Style {
                                color: Some(theme::ctp::TEXT),
                            });
                        let mut bg_color = source_bg;

                        if instr.kind == loader::InstructionKind::BlockLabel {
                            text_widget = text_widget.font(Font {
                                weight: iced::font::Weight::Bold,
                                ..Font::MONOSPACE
                            });
                            let mut label_bg = block_color;
                            label_bg.a = 0.10;
                            bg_color = label_bg;
                        } else if instr.kind == loader::InstructionKind::Terminator {
                            text_widget = text_widget.style(|_| iced::widget::text::Style {
                                color: Some(theme::ctp::RED),
                            });
                        }

                        let text_block = container(text_widget)
                            .width(Length::Fill)
                            .padding(Padding {
                                top: 2.0,
                                bottom: 2.0,
                                left: 12.0,
                                right: 8.0,
                            })
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_color)),
                                border: Border::default(),
                                ..Default::default()
                            });

                        // 4. Render
                        instr_col = instr_col.push(
                            mouse_area(
                                tooltip(
                                    row![block_indicator, text_block]
                                        .spacing(8)
                                        .align_y(iced::Alignment::Center),
                                    text(&instr.description).size(12).style(|_| {
                                        iced::widget::text::Style {
                                            color: Some(theme::ctp::SUBTEXT0),
                                        }
                                    }),
                                    tooltip::Position::Right,
                                )
                                .style(|_| theme::tooltip_box()),
                            )
                            .on_enter(Message::HoverSourceLine(instr.source_line_index)),
                        );
                    }

                    let interactive_instr_col =
                        mouse_area(instr_col).on_exit(Message::HoverSourceLine(None));
                    col = col.push(interactive_instr_col);

                    container(scrollable(col))
                        .padding(16)
                        .style(|_| theme::base_panel())
                        .into()
                }
            })
        })
        .width(Length::FillPortion(3))
        .on_resize(10.0, Message::Resized);

        // ===== Main Layout =====
        let content = row![pane_grid, sidebar].spacing(0);

        column![header, content].spacing(0).into()
    }
}

pub fn main() -> iced::Result {
    let target = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("..."));
    iced::application(
        move || CompilerExplorer::new(target.clone()),
        CompilerExplorer::update,
        CompilerExplorer::view,
    )
    .title("Compiler Explorer")
    .run()
}
