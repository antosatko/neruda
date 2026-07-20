use iced::widget::operation::scroll_to;
use iced::widget::{
    Id, PaneGrid, Space, button, column, container, mouse_area, pane_grid, row, scrollable, text,
    tooltip,
};
use iced::{Background, Border, Color, Element, Font, Length, Padding, Shadow, Task};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

mod loader;
mod theme;

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
    hovered_element: Option<loader::IrElement>,

    nav_history: Vec<String>,
    current_target: Option<String>,

    collapsed_modules: HashSet<usize>,
    expanded_polymorphs: HashSet<String>,
    panes: pane_grid::State<PaneState>,
}

#[derive(Debug, Clone)]
enum Message {
    ToggleModule(usize),
    TogglePolymorphic(String),
    NavigateTo(String),
    NavigateBack,
    ClickInstruction(Option<usize>),
    HoverSourceLine(Option<usize>),
    HoverElement(Option<loader::IrElement>),
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
            nav_history: Vec::new(),
            current_target: None,
            collapsed_modules: HashSet::new(),
            expanded_polymorphs: HashSet::new(),
            panes,
            hovered_element: None,
        };

        if let Some(ref proj) = explorer.project {
            if let Some((target, details)) = proj.details.iter().next() {
                explorer.current_target = Some(target.clone());
                explorer.source_lines = details.source_lines.clone();
                explorer.ir_variables = details.ir_variables.clone();
                explorer.ir_values = details.ir_values.clone();
                explorer.ir_instructions = details.ir_instructions.clone();
                explorer.active_lines = build_line_colors(&details.ir_instructions);
                explorer.active_color_index = details.color_index;

                if let Some(line) = explorer.active_lines.keys().min().copied() {
                    return (
                        explorer,
                        scroll_to(
                            Id::new("source_scroll"),
                            scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: line.saturating_sub(2) as f32 * 20.0,
                            },
                        ),
                    );
                }
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
                Task::none()
            }
            Message::TogglePolymorphic(key) => {
                if self.expanded_polymorphs.contains(&key) {
                    self.expanded_polymorphs.remove(&key);
                } else {
                    self.expanded_polymorphs.insert(key);
                }
                Task::none()
            }
            Message::HoverElement(element) => {
                match element {
                    Some(loader::IrElement::Text(_) | loader::IrElement::Operator(_)) => (),
                    _ => self.hovered_element = element,
                }
                Task::none()
            }
            Message::NavigateTo(target) => {
                if let Some(ref proj) = self.project {
                    if let Some(details) = proj.details.get(&target) {
                        if let Some(current) = self.current_target.take() {
                            if self.nav_history.last() != Some(&current) {
                                self.nav_history.push(current);
                            }
                        }

                        self.current_target = Some(target);
                        self.source_lines = details.source_lines.clone();
                        self.ir_variables = details.ir_variables.clone();
                        self.ir_values = details.ir_values.clone();
                        self.ir_instructions = details.ir_instructions.clone();
                        self.active_lines = build_line_colors(&details.ir_instructions);
                        self.active_color_index = details.color_index;

                        if let Some(line) = self.active_lines.keys().min().copied() {
                            return scroll_to(
                                Id::new("source_scroll"),
                                scrollable::AbsoluteOffset {
                                    x: 0.0,
                                    y: line.saturating_sub(2) as f32 * 20.0,
                                },
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::NavigateBack => {
                if let Some(prev_target) = self.nav_history.pop() {
                    if let Some(ref proj) = self.project {
                        if let Some(details) = proj.details.get(&prev_target) {
                            self.current_target = Some(prev_target);
                            self.source_lines = details.source_lines.clone();
                            self.ir_variables = details.ir_variables.clone();
                            self.ir_values = details.ir_values.clone();
                            self.ir_instructions = details.ir_instructions.clone();
                            self.active_lines = build_line_colors(&details.ir_instructions);
                            self.active_color_index = details.color_index;

                            if let Some(line) = self.active_lines.keys().min().copied() {
                                return scroll_to(
                                    Id::new("source_scroll"),
                                    scrollable::AbsoluteOffset {
                                        x: 0.0,
                                        y: line.saturating_sub(2) as f32 * 20.0,
                                    },
                                );
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::ClickInstruction(Some(line)) => scroll_to(
                Id::new("source_scroll"),
                scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: line.saturating_sub(2) as f32 * 20.0,
                },
            ),
            Message::ClickInstruction(None) => Task::none(),
            Message::HoverSourceLine(line) => {
                self.hovered_source_line = line;
                Task::none()
            }
            Message::Resized(e) => {
                self.panes.resize(e.split, e.ratio);
                Task::none()
            }
            Message::ToggleVariables => {
                self.variables_expanded = !self.variables_expanded;
                Task::none()
            }
            Message::ToggleValues => {
                self.values_expanded = !self.values_expanded;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // ===== Header Bar =====
        let mut header_row = row![
            text("Compiler Explorer")
                .font(Font::MONOSPACE)
                .size(18)
                .style(|_| iced::widget::text::Style {
                    color: Some(theme::ctp::LAVENDER),
                }),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        if !self.nav_history.is_empty() {
            header_row = header_row.push(
                button(text("← Back").size(14))
                    .on_press(Message::NavigateBack)
                    .style(|_, _| button::Style {
                        background: Some(Background::Color(theme::ctp::SURFACE0)),
                        text_color: theme::ctp::TEXT,
                        border: Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: 4.0.into(),
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    })
                    .padding(Padding::new(4.0).left(8.0).right(8.0)),
            );
        }

        if let Some(target) = &self.current_target {
            header_row = header_row.push(text(format!(" — {}", target)).size(14).style(|_| {
                iced::widget::text::Style {
                    color: Some(theme::ctp::OVERLAY1),
                }
            }));
        }

        let header = container(header_row)
            .width(Length::Fill)
            .padding(Padding::new(12.0))
            .style(|_| theme::header_bar());

        // ===== Sidebar =====
        let sidebar_content = {
            let mut sidebar_col = column![
                text("Modules")
                    .size(16)
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
                        let mut obj_col = column![].spacing(4).padding(Padding::new(12.0).left);

                        for obj in &module.objects {
                            let export_icon = if obj.is_exported { "●" } else { "○" };
                            let icon_color = if obj.is_exported {
                                theme::ctp::GREEN
                            } else {
                                theme::ctp::OVERLAY0
                            };

                            let is_expanded = self
                                .expanded_polymorphs
                                .contains(&format!("{}::{}", module.name, obj.name));

                            let mut label_row = row![text(export_icon).size(10).style(move |_| {
                                iced::widget::text::Style {
                                    color: Some(icon_color),
                                }
                            }),]
                            .spacing(8)
                            .align_y(iced::Alignment::Center);

                            if obj.is_polymorphic {
                                label_row = label_row.push(
                                    text(if is_expanded { "▼" } else { "▶" }).size(10).style(
                                        |_| iced::widget::text::Style {
                                            color: Some(theme::ctp::OVERLAY1),
                                        },
                                    ),
                                );
                            }

                            label_row = label_row.push(text(&obj.name).size(13).style(|_| {
                                iced::widget::text::Style {
                                    color: if obj.is_polymorphic {
                                        Some(theme::ctp::OVERLAY1)
                                    } else {
                                        Some(theme::ctp::SUBTEXT0)
                                    },
                                }
                            }));

                            if obj.is_polymorphic {
                                obj_col = obj_col.push(
                                    button(label_row)
                                        .on_press(Message::TogglePolymorphic(format!(
                                            "{}::{}",
                                            module.name, obj.name
                                        )))
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

                                if is_expanded {
                                    let mut morph_col =
                                        column![].spacing(2).padding(Padding::new(12.0).left);

                                    for (sig, target) in &obj.morphed_versions {
                                        let sig_label = row![
                                            text("↳").size(12).style(|_| {
                                                iced::widget::text::Style {
                                                    color: Some(theme::ctp::OVERLAY0),
                                                }
                                            }),
                                            text(sig).size(12).font(Font::MONOSPACE).style(|_| {
                                                iced::widget::text::Style {
                                                    color: Some(theme::ctp::SUBTEXT0),
                                                }
                                            }),
                                        ]
                                        .spacing(6)
                                        .align_y(iced::Alignment::Center);

                                        morph_col = morph_col.push(
                                            button(sig_label)
                                                .on_press(Message::NavigateTo(target.clone()))
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
                                                .padding(Padding::new(2.0))
                                                .width(Length::Fill),
                                        );
                                    }
                                    obj_col = obj_col.push(morph_col);
                                }
                            } else {
                                obj_col = obj_col.push(
                                    button(label_row)
                                        .on_press(Message::NavigateTo(format!(
                                            "{}::{}",
                                            module.name, obj.name
                                        )))
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
            .padding(8)
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
                            color_offset.map(|&offset| {
                                theme::accent_hover(self.active_color_index + offset)
                            })
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

                        let line_container = container(
                            row![
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
                                container(
                                    Space::new().width(Length::Fixed(1.0)).height(Length::Fill)
                                )
                                .style(|_| {
                                    container::Style {
                                        background: Some(Background::Color(theme::ctp::SURFACE0)),
                                        ..Default::default()
                                    }
                                }),
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
                        .style(move |_| theme::code_line_bg(line_bg));

                        col = col.push(mouse_area(line_container).on_enter(
                            if color_offset.is_some() {
                                Message::HoverSourceLine(Some(index))
                            } else {
                                Message::HoverSourceLine(None)
                            },
                        ));
                    }

                    let interactive_col = mouse_area(col).on_exit(Message::HoverSourceLine(None));
                    container(scrollable(interactive_col).id(Id::new("source_scroll")))
                        .padding(12)
                        .style(|_| theme::base_panel())
                }

                PaneState::Ir => {
                    let mut col = column![].spacing(12);

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
                            border: Border {
                                color: Color::TRANSPARENT,
                                width: 0.0,
                                radius: 4.0.into(),
                            },
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
                            .padding(Padding::new(6.0).left(8.0).right(8.0))
                            .style(|_| theme::table_header()),
                        ]
                        .spacing(2);

                        for v in &self.ir_variables {
                            table = table.push(
                                container(
                                    row![
                                        text(format!("{}", v.id.id()))
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
                            border: Border {
                                color: Color::TRANSPARENT,
                                width: 0.0,
                                radius: 4.0.into(),
                            },
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
                            .padding(Padding::new(6.0).left(8.0).right(8.0))
                            .style(|_| theme::table_header()),
                        ]
                        .spacing(2);

                        for v in &self.ir_values {
                            table = table.push(
                                container(
                                    row![
                                        text(format!("{}", v.id.id()))
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
                        let offset = instr
                            .source_line_index
                            .and_then(|idx| self.active_lines.get(&idx).copied())
                            .unwrap_or(0);
                        let color_idx = self.active_color_index + offset;
                        let mut bg_color = theme::accent_medium(color_idx);

                        if let Some(hover) = self.hovered_source_line {
                            if Some(hover) != instr.source_line_index {
                                bg_color = theme::accent_dim(color_idx);
                            } else {
                                bg_color = theme::accent_hover(color_idx);
                            }
                        }

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

                        let mut row_content = row![].spacing(4);

                        for element in &instr.elements {
                            let is_highlighted = Some(element) == self.hovered_element.as_ref();

                            let text_color = if instr.kind == loader::InstructionKind::Terminator {
                                theme::ctp::RED
                            } else if is_highlighted {
                                theme::ctp::YELLOW
                            } else {
                                theme::ctp::TEXT
                            };

                            let base_text = text(element.stringify().to_string())
                                .font(if instr.kind == loader::InstructionKind::BlockLabel {
                                    Font {
                                        weight: iced::font::Weight::Bold,
                                        ..Font::MONOSPACE
                                    }
                                } else {
                                    Font::MONOSPACE
                                })
                                .size(13)
                                .style(move |_| iced::widget::text::Style {
                                    color: Some(text_color),
                                });

                            let element_widget: Element<'_, Message> = match element {
                                loader::IrElement::Variable { id } => {
                                    let hover_text = self
                                        .ir_variables
                                        .iter()
                                        .find(|v| v.id == *id)
                                        .map(|v| format!("{}: {}", v.identifier, v.ty))
                                        .unwrap_or_else(|| format!("var_{}", id.id()));

                                    tooltip(
                                        base_text,
                                        container(text(hover_text).size(12)).padding(4),
                                        tooltip::Position::Top,
                                    )
                                    .style(|_| theme::tooltip_box())
                                    .into()
                                }
                                loader::IrElement::Value { id } => {
                                    let hover_text = self
                                        .ir_values
                                        .iter()
                                        .find(|v| v.id == *id)
                                        .map(|v| format!("type: {}", v.ty))
                                        .unwrap_or_else(|| format!("val_{}", id.id()));

                                    tooltip(
                                        base_text,
                                        container(text(hover_text).size(12)).padding(4),
                                        tooltip::Position::Top,
                                    )
                                    .style(|_| theme::tooltip_box())
                                    .into()
                                }
                                loader::IrElement::Function { id } => {
                                    if let Some(ref proj) = self.project {
                                        if let Some(target_name) = proj.function_map.get(&id.id()) {
                                            button(base_text)
                                                .on_press(Message::NavigateTo(target_name.clone()))
                                                .style(|_, _| button::Style {
                                                    background: None,
                                                    text_color: theme::ctp::SKY,
                                                    border: Border::default(),
                                                    shadow: Shadow::default(),
                                                    snap: false,
                                                })
                                                .padding(0)
                                                .into()
                                        } else {
                                            base_text.into()
                                        }
                                    } else {
                                        base_text.into()
                                    }
                                }
                                _ => base_text.into(),
                            };

                            let interactive_widget = mouse_area(element_widget)
                                .on_enter(Message::HoverElement(Some(element.clone())))
                                .on_exit(Message::HoverElement(None));

                            row_content = row_content.push(interactive_widget);
                        }

                        if instr.kind == loader::InstructionKind::BlockLabel {
                            let mut label_bg = block_color;
                            label_bg.a = 0.10;
                            bg_color = label_bg;
                        }

                        let instruction_container = container(row_content)
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

                        let mut interaction_area = mouse_area(
                            tooltip(
                                row![block_indicator, instruction_container]
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
                        .on_enter(Message::HoverSourceLine(instr.source_line_index));

                        if let Some(line_idx) = instr.source_line_index {
                            interaction_area = interaction_area
                                .on_press(Message::ClickInstruction(Some(line_idx)));
                        }

                        instr_col = instr_col.push(interaction_area);
                    }

                    let interactive_instr_col =
                        mouse_area(instr_col).on_exit(Message::HoverSourceLine(None));
                    col = col.push(interactive_instr_col);

                    container(scrollable(col))
                        .padding(12)
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
