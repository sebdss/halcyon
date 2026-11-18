/* Configuration du site Halcyon.
   Modifiez ce fichier pour changer le dépôt, le canal affiché ou les fichiers proposés :
   aucune autre partie du site n'a besoin d'être touchée. */
window.HALCYON_SITE = {
  // Dépôt GitHub qui publie les versions (GitHub Releases).
  repo: "sebdss/halcyon",

  // true : proposer aussi les préversions (alpha, beta). Tant qu'aucune version stable
  // n'existe, laisser à true, sinon la section Téléchargement n'affichera aucune version.
  includePrereleases: true,

  // Fichiers proposés pour chaque système. `match` est comparé au nom des fichiers de la
  // version (expressions régulières). Le premier fichier de chaque liste est le choix principal.
  // Une entrée sans fichier correspondant dans la version n'est pas affichée.
  // Textes : chaîne unique, ou { fr: "…", en: "…" } pour la page française et la page anglaise.
  assets: {
    windows: [
      { match: /windows-x86_64-setup\.exe$/i,
        label: { fr: "Programme d'installation", en: "Installer" },
        note: { fr: "Recommandé · pour votre compte, sans droits administrateur", en: "Recommended · for your account, no administrator rights" } },
      { match: /windows-portable\.zip$/i,
        label: { fr: "Version portable", en: "Portable version" },
        note: { fr: "Archive .zip · données dans le dossier voisin", en: ".zip archive · data in the folder next to it" } }
    ],
    linux: [
      { match: /x86_64\.AppImage$/i,
        label: "AppImage",
        note: { fr: "Recommandé · un seul fichier, mises à jour automatiques", en: "Recommended · a single file, automatic updates" } },
      { match: /_amd64\.deb$/i,
        label: { fr: "Paquet .deb", en: ".deb package" },
        note: "Debian, Ubuntu, Mint" },
      { match: /linux-x86_64-setup$/i,
        label: { fr: "Programme d'installation", en: "Installer" },
        note: { fr: "Toutes distributions, dans ~/.local/opt", en: "Any distribution, into ~/.local/opt" } },
      { match: /linux-x86_64\.tar\.gz$/i,
        label: { fr: "Archive .tar.gz", en: ".tar.gz archive" },
        note: { fr: "Scripts install.sh et uninstall.sh", en: "install.sh and uninstall.sh scripts" } }
    ]
  },

  // Première version dont l'interface est traduite (anglais, allemand, espagnol, italien,
  // portugais). Tant que la dernière version publiée est plus ancienne, la page anglaise
  // l'indique dans la section Téléchargement. Mettre à jour si le numéro change.
  translatedFrom: "1.0.0-alpha.5",

  // Fichiers d'intégrité publiés avec chaque version.
  checksums: /^SHA256SUMS$/,
  signature: /^SHA256SUMS\.sig$/
};
