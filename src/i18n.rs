//! UI translations are data; English keys remain the canonical source strings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    French,
    Portuguese,
}
impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::French => "fr",
            Self::Portuguese => "pt-BR",
        }
    }
    pub fn from_code(code: &str) -> Self {
        match code {
            "fr" => Self::French,
            "pt-BR" => Self::Portuguese,
            _ => Self::English,
        }
    }
    pub fn text(self, key: &'static str) -> &'static str {
        match (self, key) {
            (Self::French, "Toggle focused display") => "Activer/désactiver l’écran actif",
            (Self::French, "Toggle all displays") => "Activer/désactiver tous les écrans",
            (Self::French, "Active window hint") => "Bordure de la fenêtre active",
            (Self::French, "Float focused window") => "Fenêtre active flottante",
            (Self::French, "Resize mode") => "Mode de redimensionnement",
            (Self::French, "Rotate split") => "Changer l’orientation",
            (Self::French, "Move to next display") => "Déplacer vers l’écran suivant",
            (Self::French, "Keyboard shortcuts") => "Raccourcis clavier",
            (Self::French, "Quit and restore") => "Quitter et restaurer",
            (Self::French, "Language") => "Langue",
            (Self::French, "Gap") => "Espacement",
            (Self::French, "Hint color") => "Couleur de bordure",
            (Self::French, "Cyan") => "Cyan",
            (Self::French, "Violet") => "Violet",
            (Self::French, "Orange") => "Orange",
            (Self::French, "Display") => "Écran",
            (Self::French, "On") => "Activé",
            (Self::French, "Off") => "Désactivé",
            (Self::French, "About") => "À propos",
            (Self::Portuguese, "Toggle focused display") => "Alternar monitor focado",
            (Self::Portuguese, "Toggle all displays") => "Alternar todos os monitores",
            (Self::Portuguese, "Active window hint") => "Destacar janela ativa",
            (Self::Portuguese, "Float focused window") => "Alternar janela flutuante",
            (Self::Portuguese, "Resize mode") => "Modo de redimensionamento",
            (Self::Portuguese, "Rotate split") => "Alternar orientação da divisão",
            (Self::Portuguese, "Move to next display") => "Mover ao próximo monitor",
            (Self::Portuguese, "Keyboard shortcuts") => "Atalhos de teclado",
            (Self::Portuguese, "Quit and restore") => "Sair e restaurar",
            (Self::Portuguese, "Language") => "Idioma",
            (Self::Portuguese, "Gap") => "Espaçamento",
            (Self::Portuguese, "Hint color") => "Cor do destaque",
            (Self::Portuguese, "Cyan") => "Ciano",
            (Self::Portuguese, "Violet") => "Violeta",
            (Self::Portuguese, "Orange") => "Laranja",
            (Self::Portuguese, "Display") => "Monitor",
            (Self::Portuguese, "On") => "Ativado",
            (Self::Portuguese, "Off") => "Desativado",
            (Self::Portuguese, "About") => "Sobre",
            (Self::French, "Focus window") => "Changer de fenêtre active",
            (Self::French, "Swap windows") => "Échanger les fenêtres",
            (Self::French, "Decrease/increase width") => "Réduire/augmenter la largeur",
            (Self::French, "Decrease/increase height") => "Réduire/augmenter la hauteur",
            (Self::French, "Exit resize mode") => "Quitter le redimensionnement",
            (Self::French, "Hotkey conflicts") => "Conflits de raccourcis",
            (Self::French, "Windows refusing movement") => "Fenêtres refusant le déplacement",
            (Self::French, "Native Rust + Win32 preview. Starts with tiling off. Save/Open dialogs stay floating.") => "Version préliminaire native Rust + Win32. Le pavage est désactivé au démarrage. Les dialogues Enregistrer/Ouvrir restent flottants.",
            (Self::Portuguese, "Focus window") => "Mudar janela focada",
            (Self::Portuguese, "Swap windows") => "Trocar janelas de posição",
            (Self::Portuguese, "Decrease/increase width") => "Reduzir/aumentar largura",
            (Self::Portuguese, "Decrease/increase height") => "Reduzir/aumentar altura",
            (Self::Portuguese, "Exit resize mode") => "Sair do redimensionamento",
            (Self::Portuguese, "Hotkey conflicts") => "Conflitos de atalhos",
            (Self::Portuguese, "Windows refusing movement") => "Janelas recusando movimentação",
            (Self::Portuguese, "Native Rust + Win32 preview. Starts with tiling off. Save/Open dialogs stay floating.") => "Versão inicial nativa Rust + Win32. O tiling inicia desativado. Diálogos Salvar/Abrir ficam flutuantes.",
            _ => key,
        }
    }
}
