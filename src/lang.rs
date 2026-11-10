//! Translations. Text is written in English throughout the code and looked
//! up here when shown: `t("Exit")`. Anything without a translation is shown
//! in English.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::config::LanguageChoice;

/// A column of `TABLE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English = 0,
    Spanish = 1,
    German = 2,
    French = 3,
}

static CURRENT: AtomicUsize = AtomicUsize::new(Language::English as usize);

pub fn set_language(language: Language) {
    CURRENT.store(language as usize, Ordering::Relaxed);
}

/// The language to use: the one chosen in the settings, or for "auto" the
/// one Windows is set to (`windows_language` is a Windows language id).
pub fn resolve(choice: LanguageChoice, windows_language: u16) -> Language {
    match choice {
        LanguageChoice::English => Language::English,
        LanguageChoice::Spanish => Language::Spanish,
        LanguageChoice::German => Language::German,
        LanguageChoice::French => Language::French,
        // The low ten bits identify the language, whatever the country.
        LanguageChoice::Auto => match windows_language & 0x3FF {
            0x0A => Language::Spanish,
            0x07 => Language::German,
            0x0C => Language::French,
            _ => Language::English,
        },
    }
}

/// Translates `english` into the current language.
pub fn t(english: &str) -> &str {
    translate(english, CURRENT.load(Ordering::Relaxed))
}

/// Translates `english`, then fills each `{}` in it with the next of `args`.
pub fn tf(english: &str, args: &[&str]) -> String {
    let mut text = t(english).to_string();
    for arg in args {
        text = text.replacen("{}", arg, 1);
    }
    text
}

fn translate(english: &str, column: usize) -> &str {
    TABLE
        .iter()
        .find(|row| row[0] == english)
        .map_or(english, |row| row[column])
}

/// "1 minute", "45 minutes", "2 hours".
pub fn minutes_label(minutes: u32) -> String {
    match minutes {
        1 => t("1 minute").to_string(),
        60 => t("1 hour").to_string(),
        m if m >= 120 && m % 60 == 0 => tf("{} hours", &[&(m / 60).to_string()]),
        m => tf("{} minutes", &[&m.to_string()]),
    }
}

pub fn seconds_label(seconds: u32) -> String {
    match seconds {
        s if s >= 60 && s % 60 == 0 => minutes_label(s / 60),
        s => tf("{} seconds", &[&s.to_string()]),
    }
}

/// Each row: English, Spanish, German, French.
#[rustfmt::skip]
static TABLE: &[[&str; 4]] = &[
    // --- Tray menu ---
    ["After {}", "Tras {}", "Nach {}", "Après {}"],
    ["When idle", "Al estar inactivo", "Bei Inaktivität", "En cas d'inactivité"],
    ["Countdown", "Cuenta atrás", "Countdown", "Compte à rebours"],
    ["Don't count as idle while", "No contar como inactivo mientras", "Nicht als inaktiv werten, solange", "Ne pas considérer comme inactif tant que"],
    ["Turn screen off now", "Apagar la pantalla ahora", "Bildschirm jetzt ausschalten", "Éteindre l'écran maintenant"],
    ["Keep screen on for", "Mantener la pantalla encendida durante", "Bildschirm eingeschaltet lassen für", "Garder l'écran allumé pendant"],
    ["Stop keeping screen on ({} left)", "Dejar de mantener la pantalla encendida (quedan {})", "Bildschirm nicht mehr eingeschaltet lassen (noch {})", "Ne plus garder l'écran allumé ({} restantes)"],
    ["Tray icon", "Icono de la bandeja", "Infobereichssymbol", "Icône de la zone de notification"],
    ["Start with Windows", "Iniciar con Windows", "Mit Windows starten", "Démarrer avec Windows"],
    ["Settings...", "Configuración...", "Einstellungen...", "Paramètres..."],
    ["Exit", "Salir", "Beenden", "Quitter"],
    ["None", "Ninguna", "Keiner", "Aucun"],
    ["Show on", "Mostrar en", "Anzeigen auf", "Afficher sur"],
    ["Position", "Posición", "Position", "Position"],
    ["Play a sound", "Reproducir un sonido", "Ton abspielen", "Émettre un son"],
    ["Preview", "Vista previa", "Vorschau", "Aperçu"],
    ["Music or video is playing", "Se reproduce música o vídeo", "Musik oder Video läuft", "De la musique ou une vidéo est en lecture"],
    ["A full-screen app is in front", "Hay una aplicación a pantalla completa en primer plano", "Eine Vollbild-App im Vordergrund ist", "Une application en plein écran est au premier plan"],
    ["The cursor is at a chosen screen edge", "El cursor está en un borde elegido de la pantalla", "Der Mauszeiger an einem gewählten Bildschirmrand ist", "Le curseur est sur un bord choisi de l'écran"],

    // --- Durations ---
    ["1 minute", "1 minuto", "1 Minute", "1 minute"],
    ["{} minutes", "{} minutos", "{} Minuten", "{} minutes"],
    ["1 hour", "1 hora", "1 Stunde", "1 heure"],
    ["{} hours", "{} horas", "{} Stunden", "{} heures"],
    ["{} seconds", "{} segundos", "{} Sekunden", "{} secondes"],

    // --- Status line and tooltip ---
    ["Screen is being kept on: {}", "La pantalla se mantiene encendida: {}", "Bildschirm bleibt eingeschaltet: {}", "L'écran reste allumé : {}"],
    ["Not active outside {}", "Inactivo fuera de {}", "Außerhalb von {} nicht aktiv", "Inactif en dehors de {}"],
    ["Last kept on by: {} (until {})", "Última causa de pantalla encendida: {} (hasta las {})", "Zuletzt eingeschaltet gehalten durch: {} (bis {})", "Dernière cause d'écran allumé : {} (jusqu'à {})"],
    ["\"keep screen on\" timer", "temporizador de «mantener la pantalla encendida»", "Timer „Bildschirm eingeschaltet lassen“", "minuterie « garder l'écran allumé »"],
    ["music or video playing", "música o vídeo en reproducción", "Musik oder Video läuft", "musique ou vidéo en lecture"],
    ["game controller in use", "mando de juego en uso", "Gamecontroller in Benutzung", "manette de jeu utilisée"],
    ["cursor at a screen edge", "cursor en un borde de la pantalla", "Mauszeiger am Bildschirmrand", "curseur sur un bord de l'écran"],
    ["full-screen app in front", "aplicación a pantalla completa en primer plano", "Vollbild-App im Vordergrund", "application en plein écran au premier plan"],
    ["a listed program running", "un programa de la lista en ejecución", "ein aufgeführtes Programm läuft", "un programme de la liste en cours d'exécution"],
    ["Keeping the screen on", "Manteniendo la pantalla encendida", "Bildschirm bleibt eingeschaltet", "L'écran reste allumé"],
    ["Switched off", "Desactivado", "Ausgeschaltet", "Désactivé"],
    ["{} (after {})", "{} (tras {})", "{} (nach {})", "{} (après {})"],

    // --- Notifications ---
    ["screenSleep is running here in the tray. Right-click the icon for options.", "screenSleep se está ejecutando aquí, en la bandeja. Haz clic derecho en el icono para ver las opciones.", "screenSleep läuft hier im Infobereich. Rechtsklick auf das Symbol zeigt die Optionen.", "screenSleep s'exécute ici, dans la zone de notification. Cliquez avec le bouton droit sur l'icône pour les options."],
    ["screenSleep is on.", "screenSleep está activado.", "screenSleep ist eingeschaltet.", "screenSleep est activé."],
    ["screenSleep is off.", "screenSleep está desactivado.", "screenSleep ist ausgeschaltet.", "screenSleep est désactivé."],
    ["Keeping the screen on for {}.", "La pantalla se mantendrá encendida durante {}.", "Der Bildschirm bleibt {} lang eingeschaltet.", "L'écran restera allumé pendant {}."],
    ["No longer keeping the screen on.", "La pantalla ya no se mantiene encendida.", "Der Bildschirm wird nicht mehr eingeschaltet gehalten.", "L'écran n'est plus maintenu allumé."],
    ["Problem in the settings file. {}", "Problema en el archivo de configuración. {}", "Problem in der Einstellungsdatei. {}", "Problème dans le fichier de paramètres. {}"],
    ["Couldn't set up the hotkey {}. Another program may be using it.", "No se pudo registrar el atajo {}. Puede que otro programa lo esté usando.", "Das Tastenkürzel {} konnte nicht eingerichtet werden. Möglicherweise verwendet es ein anderes Programm.", "Impossible d'enregistrer le raccourci {}. Un autre programme l'utilise peut-être."],

    // --- Actions ---
    ["Turn screen off", "Apagar la pantalla", "Bildschirm ausschalten", "Éteindre l'écran"],
    ["Lock", "Bloquear", "Sperren", "Verrouiller"],
    ["Lock and turn screen off", "Bloquear y apagar la pantalla", "Sperren und Bildschirm ausschalten", "Verrouiller et éteindre l'écran"],
    ["Start screen saver", "Iniciar el protector de pantalla", "Bildschirmschoner starten", "Lancer l'écran de veille"],
    ["Sleep", "Suspender", "Energie sparen", "Mettre en veille"],
    ["Hibernate", "Hibernar", "Ruhezustand", "Mettre en veille prolongée"],
    ["Turn screen off when idle", "Apagar la pantalla al estar inactivo", "Bildschirm bei Inaktivität ausschalten", "Éteindre l'écran en cas d'inactivité"],
    ["Lock when idle", "Bloquear al estar inactivo", "Bei Inaktivität sperren", "Verrouiller en cas d'inactivité"],
    ["Lock and turn screen off when idle", "Bloquear y apagar la pantalla al estar inactivo", "Bei Inaktivität sperren und Bildschirm ausschalten", "Verrouiller et éteindre l'écran en cas d'inactivité"],
    ["Start screen saver when idle", "Iniciar el protector de pantalla al estar inactivo", "Bei Inaktivität Bildschirmschoner starten", "Lancer l'écran de veille en cas d'inactivité"],
    ["Sleep when idle", "Suspender al estar inactivo", "Bei Inaktivität Energie sparen", "Mettre en veille en cas d'inactivité"],
    ["Hibernate when idle", "Hibernar al estar inactivo", "Bei Inaktivität in den Ruhezustand", "Mettre en veille prolongée en cas d'inactivité"],

    // --- Countdown window ---
    ["Screen turning off in", "La pantalla se apagará en", "Bildschirm wird ausgeschaltet in", "Extinction de l'écran dans"],
    ["Locking in", "Bloqueo en", "Sperren in", "Verrouillage dans"],
    ["Locking and turning screen off in", "Bloqueo y apagado de pantalla en", "Sperren und Bildschirm ausschalten in", "Verrouillage et extinction de l'écran dans"],
    ["Screen saver starting in", "El protector de pantalla se iniciará en", "Bildschirmschoner startet in", "Écran de veille dans"],
    ["Going to sleep in", "Suspensión en", "Energiesparmodus in", "Mise en veille dans"],
    ["Hibernating in", "Hibernación en", "Ruhezustand in", "Veille prolongée dans"],
    ["Move the mouse or press a key to cancel", "Mueve el ratón o pulsa una tecla para cancelar", "Zum Abbrechen Maus bewegen oder Taste drücken", "Bougez la souris ou appuyez sur une touche pour annuler"],

    // --- Choices ---
    ["Center", "Centro", "Mitte", "Centre"],
    ["Top", "Arriba", "Oben", "En haut"],
    ["Bottom", "Abajo", "Unten", "En bas"],
    ["Top left", "Arriba a la izquierda", "Oben links", "En haut à gauche"],
    ["Top right", "Arriba a la derecha", "Oben rechts", "En haut à droite"],
    ["Bottom left", "Abajo a la izquierda", "Unten links", "En bas à gauche"],
    ["Bottom right", "Abajo a la derecha", "Unten rechts", "En bas à droite"],
    ["All screens", "Todas las pantallas", "Allen Bildschirmen", "Tous les écrans"],
    ["The screen the cursor is on", "La pantalla donde está el cursor", "Dem Bildschirm mit dem Mauszeiger", "L'écran où se trouve le curseur"],
    ["The main screen", "La pantalla principal", "Dem Hauptbildschirm", "L'écran principal"],
    ["Match taskbar", "Según la barra de tareas", "Passend zur Taskleiste", "Selon la barre des tâches"],
    ["Light", "Claro", "Hell", "Clair"],
    ["Dark", "Oscuro", "Dunkel", "Sombre"],
    ["Open the menu", "Abrir el menú", "Menü öffnen", "Ouvrir le menu"],
    ["Switch on or off", "Activar o desactivar", "Ein- oder ausschalten", "Activer ou désactiver"],
    ["Keep screen on", "Mantener la pantalla encendida", "Bildschirm eingeschaltet lassen", "Garder l'écran allumé"],
    ["Nothing", "Nada", "Nichts", "Rien"],
    ["Same as Windows", "Igual que Windows", "Wie Windows", "Comme Windows"],

    // --- Settings window ---
    ["screenSleep settings", "Configuración de screenSleep", "screenSleep-Einstellungen", "Paramètres de screenSleep"],
    ["OK", "Aceptar", "OK", "OK"],
    ["Cancel", "Cancelar", "Abbrechen", "Annuler"],
    ["Apply", "Aplicar", "Übernehmen", "Appliquer"],
    ["Apply and preview", "Aplicar y previsualizar", "Übernehmen und Vorschau", "Appliquer et prévisualiser"],
    ["Open settings file", "Abrir archivo de configuración", "Einstellungsdatei öffnen", "Ouvrir le fichier de paramètres"],
    ["Some settings need fixing:", "Hay que corregir algunos ajustes:", "Einige Einstellungen müssen korrigiert werden:", "Certains paramètres doivent être corrigés :"],
    ["minutes", "minutos", "Minuten", "minutes"],
    ["seconds", "segundos", "Sekunden", "secondes"],

    ["General", "General", "Allgemein", "Général"],
    ["Switch screenSleep on", "Activar screenSleep", "screenSleep einschalten", "Activer screenSleep"],
    ["After", "Tras", "Nach", "Après"],
    ["On battery, after (0 = the same)", "Con batería, tras (0 = igual)", "Im Akkubetrieb nach (0 = gleich)", "Sur batterie, après (0 = identique)"],
    ["Only between (e.g. 22:00-07:00)", "Solo entre (p. ej. 22:00-07:00)", "Nur zwischen (z. B. 22:00-07:00)", "Seulement entre (p. ex. 22:00-07:00)"],
    ["Hold off Windows' own screen timeout", "Impedir el apagado de pantalla propio de Windows", "Windows-eigene Bildschirmabschaltung aufhalten", "Bloquer l'extinction d'écran de Windows"],
    ["Delay before \"Turn screen off now\"", "Retardo antes de «Apagar la pantalla ahora»", "Verzögerung vor „Bildschirm jetzt ausschalten“", "Délai avant « Éteindre l'écran maintenant »"],
    ["\"Keep screen on\" lasts", "«Mantener la pantalla encendida» dura", "„Bildschirm eingeschaltet lassen“ dauert", "« Garder l'écran allumé » dure"],
    ["Language", "Idioma", "Sprache", "Langue"],

    ["While music or video is playing", "Mientras se reproduce música o vídeo", "Solange Musik oder Video läuft", "Pendant la lecture de musique ou de vidéo"],
    ["Sound must last at least", "El sonido debe durar al menos", "Ton muss mindestens andauern", "Le son doit durer au moins"],
    ["Ignore sound from (e.g. discord.exe)", "Ignorar el sonido de (p. ej. discord.exe)", "Ton ignorieren von (z. B. discord.exe)", "Ignorer le son de (p. ex. discord.exe)"],
    ["While a game controller is in use", "Mientras se usa un mando de juego", "Solange ein Gamecontroller benutzt wird", "Pendant l'utilisation d'une manette de jeu"],
    ["While a full-screen app is in front", "Mientras hay una aplicación a pantalla completa", "Solange eine Vollbild-App im Vordergrund ist", "Tant qu'une application est en plein écran"],
    ["While these programs run (e.g. vlc.exe)", "Mientras se ejecutan estos programas (p. ej. vlc.exe)", "Solange diese Programme laufen (z. B. vlc.exe)", "Tant que ces programmes s'exécutent (p. ex. vlc.exe)"],
    ["While the cursor is at a chosen screen edge", "Mientras el cursor está en un borde elegido", "Solange der Mauszeiger an einem gewählten Rand ist", "Tant que le curseur est sur un bord choisi"],
    ["Edges (top, left, bottom, right)", "Bordes (top, left, bottom, right)", "Ränder (top, left, bottom, right)", "Bords (top, left, bottom, right)"],

    ["Length (0 = no countdown)", "Duración (0 = sin cuenta atrás)", "Dauer (0 = kein Countdown)", "Durée (0 = pas de compte à rebours)"],
    ["Size", "Tamaño", "Größe", "Taille"],
    ["Opacity", "Opacidad", "Deckkraft", "Opacité"],
    ["Darken the rest of the screen by", "Oscurecer el resto de la pantalla un", "Rest des Bildschirms abdunkeln um", "Assombrir le reste de l'écran de"],
    ["Fade in and out", "Aparecer y desaparecer gradualmente", "Ein- und ausblenden", "Apparition et disparition en fondu"],
    ["Rounded corners (Windows 11)", "Esquinas redondeadas (Windows 11)", "Abgerundete Ecken (Windows 11)", "Coins arrondis (Windows 11)"],
    ["Title (empty = automatic)", "Título (vacío = automático)", "Titel (leer = automatisch)", "Titre (vide = automatique)"],
    ["Show the hint below the number", "Mostrar la indicación bajo el número", "Hinweis unter der Zahl anzeigen", "Afficher l'indication sous le nombre"],
    ["Hint", "Indicación", "Hinweis", "Indication"],

    ["Colors", "Colores", "Farben", "Couleurs"],
    ["Font", "Fuente", "Schriftart", "Police"],
    ["Background", "Fondo", "Hintergrund", "Arrière-plan"],
    ["Text", "Texto", "Text", "Texte"],
    ["Number", "Número", "Zahl", "Nombre"],
    ["Pulsing outline", "Contorno pulsante", "Pulsierender Rahmen", "Contour pulsant"],
    ["Outline color", "Color del contorno", "Rahmenfarbe", "Couleur du contour"],
    ["Pulses to", "Pulsa hacia", "Pulsiert zu", "Pulse vers"],
    ["Outline thickness", "Grosor del contorno", "Rahmenstärke", "Épaisseur du contour"],
    ["One pulse takes (0 = steady)", "Un pulso dura (0 = fijo)", "Ein Puls dauert (0 = konstant)", "Une pulsation dure (0 = fixe)"],

    ["Tray and hotkeys", "Bandeja y atajos", "Infobereich und Tastenkürzel", "Zone de notification et raccourcis"],
    ["Left-click", "Clic izquierdo", "Linksklick", "Clic gauche"],
    ["Middle-click", "Clic central", "Mittelklick", "Clic du milieu"],
    ["Show notifications", "Mostrar notificaciones", "Benachrichtigungen anzeigen", "Afficher les notifications"],
    ["Timeouts in the menu (minutes)", "Tiempos del menú (minutos)", "Zeiten im Menü (Minuten)", "Durées du menu (minutes)"],
    ["Hotkey: switch on or off (e.g. Ctrl+Alt+S)", "Atajo: activar o desactivar (p. ej. Ctrl+Alt+S)", "Tastenkürzel: ein/aus (z. B. Ctrl+Alt+S)", "Raccourci : activer ou désactiver (p. ex. Ctrl+Alt+S)"],
    ["Hotkey: turn screen off now", "Atajo: apagar la pantalla ahora", "Tastenkürzel: Bildschirm jetzt ausschalten", "Raccourci : éteindre l'écran maintenant"],
    ["Hotkey: keep screen on", "Atajo: mantener la pantalla encendida", "Tastenkürzel: Bildschirm eingeschaltet lassen", "Raccourci : garder l'écran allumé"],
    ["Keep a debug log", "Guardar un registro de depuración", "Debug-Protokoll führen", "Tenir un journal de débogage"],
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Action, ClickAction, IconStyle, Position, WarningMonitor, DEFAULT_HINT};
    use crate::schema::{Kind, TABS};

    /// Text that is the same in every language.
    const UNTRANSLATED: &[&str] = &["%", "ms", "English", "Español", "Deutsch", "Français"];

    fn has(english: &str) -> bool {
        UNTRANSLATED.contains(&english) || TABLE.iter().any(|row| row[0] == english)
    }

    #[test]
    fn rows_are_complete_and_consistent() {
        for row in TABLE {
            for text in row {
                assert!(!text.is_empty(), "{row:?}");
                assert_eq!(text.matches("{}").count(), row[0].matches("{}").count(), "{row:?}");
            }
        }
        let mut english: Vec<&str> = TABLE.iter().map(|row| row[0]).collect();
        english.sort_unstable();
        let before = english.len();
        english.dedup();
        assert_eq!(english.len(), before, "an English text appears twice");
    }

    /// Finds every `t("...")` and `tf("...")` in the source and checks the
    /// text has translations, so none can be forgotten.
    #[test]
    fn everything_shown_is_translated() {
        let sources = [
            include_str!("lang.rs"),
            include_str!("win.rs"),
            include_str!("settings.rs"),
        ];
        let mut found = 0;
        for source in sources {
            let source = source.split("#[cfg(test)]").next().unwrap();
            for call in ["t(", "tf("] {
                for (at, _) in source.match_indices(call) {
                    let standalone = !source[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_');
                    let rest = source[at + call.len()..].trim_start();
                    if !standalone || !rest.starts_with('"') {
                        continue;
                    }
                    let mut text = String::new();
                    let mut chars = rest[1..].chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => break,
                            '\\' => text.extend(chars.next()),
                            c => text.push(c),
                        }
                    }
                    assert!(has(&text), "no translations for {text:?}");
                    found += 1;
                }
            }
        }
        assert!(
            found > 40,
            "the scan found only {found} texts; has the call style changed?"
        );
    }

    #[test]
    fn labels_defined_elsewhere_are_translated() {
        let mut labels = vec![DEFAULT_HINT];
        for action in Action::ALL {
            labels.extend([action.label(), action.when_idle_label(), action.countdown_title()]);
        }
        labels.extend(Position::ALL.iter().map(|v| v.label()));
        labels.extend(WarningMonitor::ALL.iter().map(|v| v.label()));
        labels.extend(IconStyle::ALL.iter().map(|v| v.label()));
        labels.extend(ClickAction::ALL.iter().map(|v| v.label()));
        for tab in TABS {
            labels.push(tab.title);
            for field in tab.fields {
                labels.push(field.label);
                match field.kind {
                    Kind::Number(unit) => labels.push(unit),
                    Kind::Choice(options) => labels.extend(options().iter().map(|(_, label)| *label)),
                    _ => {}
                }
            }
        }
        for label in labels {
            assert!(has(label), "no translations for {label:?}");
        }
    }

    #[test]
    fn translates_and_falls_back() {
        assert_eq!(translate("Exit", Language::German as usize), "Beenden");
        assert_eq!(translate("Exit", Language::English as usize), "Exit");
        assert_eq!(
            translate("Not in the table", Language::French as usize),
            "Not in the table"
        );
    }

    #[test]
    fn picks_the_language() {
        assert_eq!(resolve(LanguageChoice::German, 0x0409), Language::German);
        assert_eq!(resolve(LanguageChoice::Auto, 0x0409), Language::English); // en-US
        assert_eq!(resolve(LanguageChoice::Auto, 0x080A), Language::Spanish); // es-MX
        assert_eq!(resolve(LanguageChoice::Auto, 0x0C07), Language::German); // de-AT
        assert_eq!(resolve(LanguageChoice::Auto, 0x0C0C), Language::French); // fr-CA
        assert_eq!(resolve(LanguageChoice::Auto, 0x0411), Language::English); // ja-JP
    }

    #[test]
    fn durations() {
        // Tests share the process-wide language, so this one leaves it alone.
        assert_eq!(minutes_label(1), "1 minute");
        assert_eq!(minutes_label(45), "45 minutes");
        assert_eq!(minutes_label(60), "1 hour");
        assert_eq!(minutes_label(90), "90 minutes");
        assert_eq!(minutes_label(120), "2 hours");
        assert_eq!(seconds_label(30), "30 seconds");
        assert_eq!(seconds_label(120), "2 minutes");
    }
}
