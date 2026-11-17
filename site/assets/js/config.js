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
  assets: {
    windows: [
      { match: /windows-x86_64-setup\.exe$/i, label: "Programme d'installation", note: "Recommandé · pour votre compte, sans droits administrateur" },
      { match: /windows-portable\.zip$/i, label: "Version portable", note: "Archive .zip · données dans le dossier voisin" }
    ],
    linux: [
      { match: /x86_64\.AppImage$/i, label: "AppImage", note: "Recommandé · un seul fichier, mises à jour automatiques" },
      { match: /_amd64\.deb$/i, label: "Paquet .deb", note: "Debian, Ubuntu, Mint" },
      { match: /linux-x86_64-setup$/i, label: "Programme d'installation", note: "Toutes distributions, dans ~/.local/opt" },
      { match: /linux-x86_64\.tar\.gz$/i, label: "Archive .tar.gz", note: "Scripts install.sh et uninstall.sh" }
    ]
  },

  // Fichiers d'intégrité publiés avec chaque version.
  checksums: /^SHA256SUMS$/,
  signature: /^SHA256SUMS\.sig$/
};
