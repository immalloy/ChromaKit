#include "audio/chromatic_generator.hpp"

#include <nfd.h>

#include <FL/Fl.H>
#include <FL/Fl_Button.H>
#include <FL/Fl_Check_Button.H>
#include <FL/Fl_Choice.H>
#include <FL/Fl_Double_Window.H>
#include <FL/Fl_Input.H>
#include <FL/Fl_Progress.H>
#include <FL/Fl_Box.H>

#ifdef _WIN32
#include <FL/platform.H>
#include <dwmapi.h>
#include <windows.h>
#include "../resources/resource.h"
#endif

#include <cstdlib>
#include <cerrno>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <string>

using namespace chromakit;

namespace {

int note_index(const char* value) {
    static const char* notes[] = {"C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"};
    if (!value) throw std::runtime_error("invalid starting note");
    for (int i = 0; i < 12; ++i) if (std::string(value) == notes[i]) return i;
    throw std::runtime_error("invalid starting note");
}

int parse_int(const char* value, const char* name) {
    if (!value || *value == '\0') throw std::runtime_error(std::string("invalid ") + name);
    errno = 0;
    char* end = nullptr;
    const long parsed = std::strtol(value, &end, 10);
    if (errno == ERANGE || !end || *end != '\0' || parsed < std::numeric_limits<int>::min() ||
        parsed > std::numeric_limits<int>::max()) {
        throw std::runtime_error(std::string("invalid ") + name);
    }
    return static_cast<int>(parsed);
}

#ifdef _WIN32
bool windows_dark_mode() {
    DWORD use_light_theme = 1;
    DWORD size = sizeof(use_light_theme);
    const auto result = RegGetValueW(
        HKEY_CURRENT_USER,
        L"Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        L"AppsUseLightTheme",
        RRF_RT_REG_DWORD,
        nullptr,
        &use_light_theme,
        &size);
    return result == ERROR_SUCCESS && use_light_theme == 0;
}
#endif

double parse_double(const char* value, const char* name) {
    if (!value || *value == '\0') throw std::runtime_error(std::string("invalid ") + name);
    errno = 0;
    char* end = nullptr;
    const double parsed = std::strtod(value, &end);
    if (errno == ERANGE || !end || *end != '\0') throw std::runtime_error(std::string("invalid ") + name);
    return parsed;
}

int run_generation(const GeneratorSettings& settings) {
    const auto report = generate_chromatic(settings, [](int percent, const std::string& message) {
        std::cout << percent << "% " << message << '\n';
        std::cout.flush();
    });
    for (const auto& sample : report.samples) {
        std::cout << sample.path.filename().string() << ": " << sample.pitch.hz << " Hz (confidence " << sample.pitch.confidence << ")\n";
    }
    std::cout << "Generated " << report.output_notes << " notes: " << report.chromatic_path.string() << '\n';
    return 0;
}

struct Ui {
    Fl_Input* folder = nullptr;
    Fl_Choice* note = nullptr;
    Fl_Choice* octave = nullptr;
    Fl_Input* range = nullptr;
    Fl_Input* gap = nullptr;
    Fl_Check_Button* pitched = nullptr;
    Fl_Check_Button* dump = nullptr;
    Fl_Progress* progress = nullptr;
    Fl_Box* status = nullptr;
};

std::string choose_folder_native(const std::string& initial_folder) {
    if (NFD_Init() != NFD_OKAY) return {};

    nfdchar_t* selected_path = nullptr;
    const nfdresult_t result = NFD_PickFolder(
        &selected_path, initial_folder.empty() ? nullptr : initial_folder.c_str());

    std::string selected;
    if (result == NFD_OKAY && selected_path) selected = selected_path;
    if (selected_path) NFD_FreePath(selected_path);
    NFD_Quit();
    return selected;
}

void browse_cb(Fl_Widget*, void* raw) {
    auto* ui = static_cast<Ui*>(raw);
    const std::string selected = choose_folder_native(ui->folder->value());
    if (!selected.empty()) {
        ui->folder->value(selected.c_str());
    }
}

void generate_cb(Fl_Widget*, void* raw) {
    auto* ui = static_cast<Ui*>(raw);
    try {
        GeneratorSettings settings;
        settings.sample_folder = ui->folder->value();
        settings.start_midi = note_index(ui->note->text()) + 12 * parse_int(ui->octave->text(), "octave") + 12;
        settings.range = parse_int(ui->range->value(), "range");
        settings.gap_seconds = parse_double(ui->gap->value(), "gap");
        settings.pitched = ui->pitched->value() != 0;
        settings.dump_individual = ui->dump->value() != 0;
        ui->progress->value(0);
        generate_chromatic(settings, [&](int percent, const std::string& message) {
            ui->progress->value(percent);
            ui->status->label(message.c_str());
            Fl::check();
        });
        ui->status->label("Complete");
        ui->progress->value(100);
    } catch (const std::exception& error) {
        ui->status->label((std::string("Error: ") + error.what()).c_str());
    }
}

int run_gui(int argc, char** argv) {
    Fl::scheme("gtk+");
#ifdef _WIN32
    const bool dark_mode = windows_dark_mode();
    if (dark_mode) {
        Fl::background(31, 31, 31);
        Fl::background2(47, 47, 47);
        Fl::foreground(235, 235, 235);
        Fl::set_color(FL_SELECTION_COLOR, 70, 145, 230);
    }
#endif
    auto* window = new Fl_Double_Window(600, 390, "Chromakit");
#ifdef _WIN32
    const HICON icon = LoadIconW(GetModuleHandleW(nullptr), MAKEINTRESOURCEW(IDI_CHROMAKIT));
    if (icon) window->icons(icon, icon);
#endif
    Ui ui;
    auto* title = new Fl_Box(20, 18, 560, 36, "Chromakit");
    title->labelfont(FL_BOLD); title->labelsize(25);
    new Fl_Box(25, 78, 110, 28, "Sample folder:");
    ui.folder = new Fl_Input(145, 78, 345, 28);
    auto* browse = new Fl_Button(500, 78, 75, 28, "Browse"); browse->callback(browse_cb, &ui);
    new Fl_Box(25, 125, 100, 28, "Starting note:");
    ui.note = new Fl_Choice(125, 125, 120, 28);
    for (const char* name : {"C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"}) ui.note->add(name);
    ui.note->value(0);
    new Fl_Box(275, 125, 60, 28, "Octave:");
    ui.octave = new Fl_Choice(340, 125, 120, 28);
    for (const char* octave : {"0", "1", "2", "3", "4", "5", "6", "7", "8", "9"}) ui.octave->add(octave);
    ui.octave->value(2);
    new Fl_Box(25, 170, 100, 28, "Range:"); ui.range = new Fl_Input(125, 170, 120, 28); ui.range->value("24");
    new Fl_Box(275, 170, 100, 28, "Gap (seconds):"); ui.gap = new Fl_Input(380, 170, 120, 28); ui.gap->value("0.1");
    ui.pitched = new Fl_Check_Button(25, 220, 200, 28, "Pitch samples"); ui.pitched->value(1);
    ui.dump = new Fl_Check_Button(250, 220, 300, 28, "Export individual files"); ui.dump->value(1);
    auto* generate = new Fl_Button(25, 265, 550, 34, "Generate Chromatic"); generate->callback(generate_cb, &ui);
    ui.progress = new Fl_Progress(25, 315, 550, 22); ui.progress->minimum(0); ui.progress->maximum(100);
    ui.status = new Fl_Box(25, 345, 550, 25, "Ready"); ui.status->align(FL_ALIGN_LEFT | FL_ALIGN_INSIDE);
    window->end();
    window->show(argc, argv);
#ifdef _WIN32
    const HWND handle = fl_win32_xid(window);
    if (handle) {
        if (icon) {
            SendMessageW(handle, WM_SETICON, ICON_BIG, reinterpret_cast<LPARAM>(icon));
            SendMessageW(handle, WM_SETICON, ICON_SMALL, reinterpret_cast<LPARAM>(icon));
        }
        const BOOL use_dark_title_bar = dark_mode ? TRUE : FALSE;
        if (DwmSetWindowAttribute(handle, 20, &use_dark_title_bar, sizeof(use_dark_title_bar)) != S_OK) {
            DwmSetWindowAttribute(handle, 19, &use_dark_title_bar, sizeof(use_dark_title_bar));
        }
    }
#endif
    return Fl::run();
}

}

int main(int argc, char** argv) {
    try {
        if (argc == 3 && std::string(argv[1]) == "--detect") {
            const auto estimate = detect_pitch(read_wav(argv[2]));
            std::cout << estimate.hz << " " << estimate.confidence << '\n';
            return 0;
        }
        if (argc >= 6 && std::string(argv[1]) == "--cli") {
            GeneratorSettings settings;
            settings.sample_folder = argv[2];
            settings.start_midi = note_index(argv[3]) + 12 * parse_int(argv[4], "octave") + 12;
            settings.range = parse_int(argv[5], "range");
            settings.gap_seconds = argc >= 7 ? parse_double(argv[6], "gap") : 0.1;
            settings.pitched = true;
            settings.dump_individual = true;
            return run_generation(settings);
        }
        return run_gui(argc, argv);
    } catch (const std::exception& error) {
        std::cerr << "Chromakit: " << error.what() << '\n';
        return 1;
    }
}
