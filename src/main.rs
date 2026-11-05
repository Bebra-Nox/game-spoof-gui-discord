#![windows_subsystem = "windows"]

use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DET_URL: &str = "https://discord.com/api/v9/applications/detectable";
const FAKE_ARG: &str = "--fake-window";

#[derive(Clone, Debug)]
struct Game {
    name: String,
    exe: String,
}

#[derive(Default)]
struct AppState {
    games: Vec<Game>,
    filtered: Vec<Game>,
    search_query: String,
    selected_index: Option<usize>,
    minutes: String,
    running: bool,
    fake_child: Option<std::process::Child>,
    fake_path: Option<PathBuf>,
    start_time: Option<Instant>,
    duration: Option<Duration>,
    status_message: String,
    loading: bool,
    warning_accepted: bool,
    show_warning: bool,
}

struct SpoofApp {
    state: Arc<Mutex<AppState>>,
}

impl SpoofApp {
    fn new() -> Self {
        let state = Arc::new(Mutex::new(AppState {
            minutes: "15".to_string(),
            show_warning: true,
            ..Default::default()
        }));

        let state_clone = Arc::clone(&state);
        std::thread::spawn(move || {
            load_games(state_clone);
        });

        Self { state }
    }
}

fn safe_exe_name(raw: &str) -> String {
    let name = raw
        .replace('>', "")
        .split(|c| c == '/' || c == '\\')
        .last()
        .unwrap_or("game.exe")
        .to_string();

    let name = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect::<String>();

    let name = name.trim();
    if name.is_empty() {
        return "game.exe".to_string();
    }

    if !name.to_lowercase().ends_with(".exe") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

fn load_games(state: Arc<Mutex<AppState>>) {
    {
        let mut s = state.lock().unwrap();
        s.loading = true;
        s.status_message = "Загружаю список игр с Discord...".to_string();
    }

    let builtin = vec![
        ("Cyberpunk 2077", "Cyberpunk2077.exe"),
        ("Elden Ring", "eldenring.exe"),
        ("Baldur's Gate 3", "bg3.exe"),
        ("Grand Theft Auto V", "GTA5.exe"),
        ("Minecraft", "Minecraft.exe"),
        ("Counter-Strike 2", "cs2.exe"),
        ("Dota 2", "dota2.exe"),
        ("Palworld", "Palworld-Win64-Shipping.exe"),
        ("Starfield", "Starfield.exe"),
        ("Hogwarts Legacy", "HogwartsLegacy.exe"),
    ];

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();

    let result = client
        .get(DET_URL)
        .header("User-Agent", "Mozilla/5.0")
        .send();

    let games = match result {
        Ok(response) => {
            if let Ok(data) = response.json::<Vec<serde_json::Value>>() {
                let mut games = Vec::new();
                for app in data {
                    if let Some(name) = app.get("name").and_then(|n| n.as_str()) {
                        if let Some(exes) = app.get("executables").and_then(|e| e.as_array()) {
                            for exe in exes {
                                if let Some(os) = exe.get("os").and_then(|o| o.as_str()) {
                                    if os == "win32" {
                                        if let Some(exe_name) = exe.get("name").and_then(|n| n.as_str()) {
                                            let is_launcher = exe
                                                .get("is_launcher")
                                                .and_then(|l| l.as_bool())
                                                .unwrap_or(false);
                                            if !is_launcher {
                                                games.push(Game {
                                                    name: name.to_string(),
                                                    exe: safe_exe_name(exe_name),
                                                });
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if games.is_empty() {
                    None
                } else {
                    Some(games)
                }
            } else {
                None
            }
        }
        Err(_) => None,
    };

    let mut s = state.lock().unwrap();
    if let Some(g) = games {
        s.games = g;
        s.status_message = format!("Загружено {} игр с Discord", s.games.len());
    } else {
        s.games = builtin
            .into_iter()
            .map(|(n, e)| Game {
                name: n.to_string(),
                exe: e.to_string(),
            })
            .collect();
        s.status_message = "Используется встроенный список (Discord недоступен)".to_string();
    }
    s.loading = false;
    filter_games(&mut s);
}

fn filter_games(state: &mut AppState) {
    let query = state.search_query.trim().to_lowercase();
    state.filtered = if query.is_empty() {
        state.games.clone()
    } else {
        state
            .games
            .iter()
            .filter(|g| {
                g.name.to_lowercase().contains(&query) || g.exe.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    };
}

impl eframe::App for SpoofApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut state = self.state.lock().unwrap();
                // Если фейковый процесс завершился сам (закрыли его окно крестиком) - убираем состояние
        if state.running {
            if let Some(child) = state.fake_child.as_mut() {
                if let Ok(Some(_)) = child.try_wait() {
                    stop_fake_process(&mut state);
                    state.status_message =
                        "Фейковый процесс завершился, копия удалена.".to_string();
                }
            }
        }
        // Если закрывают основное окно при работающем фейке - сначала останавливаем фейк
        if state.running && ctx.input(|i| i.viewport().close_requested()) {
            stop_fake_process(&mut state);
        }

        if state.show_warning && !state.warning_accepted {
            egui::Window::new("Предупреждение")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.heading("⚠️ ВНИМАНИЕ: РИСК БЛОКИРОВКИ ДОСТУПА К КВЕСТАМ");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            ui.label(
                                "С 26 августа 2026 Discord приостанавливает доступ к Quests \
                                за автоматизацию (вы сами видели сообщение \"Quest Access Suspended\"). \
                                Эта программа имитирует запуск игры, копируя свой exe под именем игры, - \
                                это нарушение условий Discord (\"Inauthentic Engagement\").\n\n\
                                Дополнительные риски именно этого подхода:\n\
                                - Самозапуск копии под чужим именем похож на поведение вирусов: \
                                антивирус/Defender может удалить файл или заблокировать запуск.\n\
                                - Discord теоретически может проверять цифровую подпись и путь процесса.\n\
                                - Работает ТОЛЬКО для квестов \"Поиграйте в игру N минут\" (PLAY_ON_DESKTOP).\n\n\
                                ВРУЧНУЮ остаются: принять квест, дождаться таймера, забрать награду.\n\
                                Всё исключительно на ваш страх и риск.",
                            );
                        });
                    ui.separator();
                    ui.checkbox(&mut state.warning_accepted, "Я понимаю риски и принимаю их на себя");
                    ui.horizontal(|ui| {
                        if state.warning_accepted {
                            if ui.button("Принять и продолжить").clicked() {
                                state.show_warning = false;
                            }
                        } else {
                            ui.add_enabled(false, egui::Button::new("Принять и продолжить"));
                        }
                        if ui.button("Выйти").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Имитация запуска игры (имя процесса)");
            ui.label(&state.status_message);
            ui.add_space(10.0);

            ui.label("Поиск игры:");
            let search_changed = ui
                .add(egui::TextEdit::singleline(&mut state.search_query).hint_text("Введите название..."))
                .changed();
            if search_changed {
                filter_games(&mut state);
            }
            ui.label(format!("Найдено: {}", state.filtered.len()));

            ui.add_space(5.0);

            // --- ИСПРАВЛЕНИЕ E0502: собираем клик в локальную переменную ---
            let mut clicked_index: Option<usize> = None;
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for (i, game) in state.filtered.iter().enumerate() {
                        let selected = state.selected_index == Some(i);
                        let response = ui.selectable_label(
                            selected,
                            format!("{}   ({})", game.name, game.exe),
                        );
                        if response.clicked() {
                            clicked_index = Some(i);
                        }
                    }
                });

            // Применяем изменения ПОСЛЕ выхода из цикла (borrow больше не конфликтует)
            if let Some(i) = clicked_index {
                if let Some(game) = state.filtered.get(i) {
                    state.status_message = format!("Выбрано: {} ({})", game.name, game.exe);
                }
                state.selected_index = Some(i);
            }

            ui.add_space(10.0);
            ui.label("Минут держать запущенной:");
            ui.add(egui::TextEdit::singleline(&mut state.minutes).desired_width(100.0));

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!state.running, egui::Button::new("▶ Запустить фейковый процесс"))
                    .clicked()
                {
                    start_fake_process(&mut state);
                }
                if ui
                    .add_enabled(state.running, egui::Button::new("⏹ Остановить и удалить копию"))
                    .clicked()
                {
                    stop_fake_process(&mut state);
                }
            });

            ui.add_space(10.0);
            if let (Some(start), Some(dur)) = (state.start_time, state.duration) {
                let elapsed = start.elapsed();
                let progress = elapsed.as_secs_f32() / dur.as_secs_f32();
                ui.add(egui::ProgressBar::new(progress.clamp(0.0, 1.0)));

                let remaining = dur.saturating_sub(elapsed);
                let mins = remaining.as_secs() / 60;
                let secs = remaining.as_secs() % 60;
                ui.label(format!("Осталось: {:02}:{:02}", mins, secs));

                if elapsed >= dur && state.running {
                    stop_fake_process(&mut state);
                    state.status_message = "Время вышло! Заберите награду ВРУЧНУЮ.".to_string();
                }
            } else {
                ui.add(egui::ProgressBar::new(0.0));
                ui.label("Осталось: --:--");
            }

            ui.add_space(10.0);
            ui.label(&state.status_message);
        });

        if state.running {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn start_fake_process(state: &mut AppState) {
    if let Some(idx) = state.selected_index {
        if let Some(game) = state.filtered.get(idx).cloned() {
            let exe_path = std::env::current_exe().unwrap();
            let games_dir = exe_path.parent().unwrap().join("games");
            fs::create_dir_all(&games_dir).unwrap();

            let dst = games_dir.join(&game.exe);
            if dst.exists() {
                let _ = fs::remove_file(&dst);
            }

            match fs::copy(&exe_path, &dst) {
                Ok(_) => {
                    match Command::new(&dst)
                        .arg(FAKE_ARG)
                        .arg(&game.name)
                        .current_dir(&games_dir)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                    {
                        Ok(child) => {
                            let pid = child.id();
                            state.fake_child = Some(child);
                            state.fake_path = Some(dst);
                            state.running = true;
                            state.start_time = Some(Instant::now());
                            state.duration = Some(Duration::from_secs(
                                state.minutes.parse::<u64>().unwrap_or(15) * 60,
                            ));
                            state.status_message = format!(
                                "Процесс {} запущен (PID {}). Discord должен показать \"Playing {}\" через ~10-30 сек.",
                                game.exe, pid, game.name
                            );
                        }
                        Err(e) => {
                            state.status_message = format!("Ошибка запуска: {}", e);
                        }
                    }
                }
                Err(e) => {
                    state.status_message = format!("Ошибка копирования: {}", e);
                }
            }
        }
    } else {
        state.status_message = "Выберите игру из списка".to_string();
    }
}

fn stop_fake_process(state: &mut AppState) {
    if let Some(mut child) = state.fake_child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(path) = &state.fake_path {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
    state.fake_path = None;
    state.running = false;
    state.start_time = None;
    state.duration = None;
    state.status_message = "Остановлено, копия удалена".to_string();
}

// --- Режим процесса-копии: маленькое окно с именем игры ---
struct FakeWindowApp {
    title: String,
}

impl eframe::App for FakeWindowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.heading(egui::RichText::new(&self.title).size(20.0).strong());
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("fake game process is running")
                        .color(egui::Color32::GRAY),
                );
            });
        });
    }
}

fn fake_window_main(title: &str) -> eframe::Result<()> {
    let title_owned = title.to_string();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([340.0, 150.0])
            .with_resizable(false)
            .with_title(title),
        ..Default::default()
    };
    eframe::run_native(
        title,
        native_options,
        Box::new(move |_cc| Ok(Box::new(FakeWindowApp { title: title_owned }))),
    )
}

fn main() -> eframe::Result<()> {
    // Режим копии: запущены с --fake-window "Name" -> показываем ТОЛЬКО маленькое окно
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == FAKE_ARG {
        return fake_window_main(&args[2]);
    }

    // Обычный режим: основное окно приложения
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([660.0, 640.0])
            .with_min_inner_size([660.0, 640.0])
            .with_max_inner_size([660.0, 640.0])
            .with_title("Game Spoof GUI (Rust)"),
        ..Default::default()
    };

    eframe::run_native(
        "Game Spoof GUI",
        native_options,
        Box::new(|_cc| Ok(Box::new(SpoofApp::new()))),
    )
}