import { defineConfig } from "vitepress";

export default defineConfig({
  lang: "en",
  title: "Tamaya",
  description:
    "Deploy apps, not containers. One VPS. Many apps.",

  themeConfig: {
    search: {
      provider: "local",
    },

    nav: [
      { text: "Guide", link: "/guide/" },
      { text: "Reference", link: "/reference/" },
    ],

    sidebar: {
      "/guide/": [
        {
          text: "Getting Started",
          items: [
            { text: "Overview", link: "/guide/" },
            { text: "Why Tamaya", link: "/guide/why" },
            { text: "Architecture", link: "/guide/architecture" },
            { text: "Quick Start", link: "/guide/quickstart" },
            { text: "Caveats", link: "/guide/caveats" },
          ],
        },
        {
          text: "Operations",
          items: [
            { text: "Deploy", link: "/guide/deploy" },
            { text: "Publish", link: "/guide/publish" },
            { text: "Environment Variables", link: "/guide/environment" },
            { text: "Configuration", link: "/guide/config" },
            { text: "Health Checks", link: "/guide/health-check" },
            { text: "Maintenance Mode", link: "/guide/maintenance" },
          ],
        },
        {
          text: "Internals",
          items: [
            { text: "Blue-Green Deploy", link: "/guide/blue-green" },
          ],
        },
      ],
      "/reference/": [
        {
          text: "Reference",
          items: [
            { text: "CLI Commands", link: "/reference/" },
            { text: "Configuration", link: "/reference/tamaya-toml" },
            { text: "Directory Layout", link: "/reference/directory-structure" },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: "github", link: "https://github.com/bhbs/tamaya" },
    ],
  },
});
