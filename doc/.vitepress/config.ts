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
      { text: "Articles", link: "/articles/" },
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
      "/articles/": [
        {
          text: "Articles",
          items: [
            { text: "Overview", link: "/articles/" },
            { text: "Why Indie Developers Should Consider a Single VPS", link: "/articles/why-single-vps" },
            { text: "How Far Can Linux Go as an Application Platform Without Containers?", link: "/articles/linux-application-platform" },
            { text: "How Tamaya Turns a Linux Server into a Deployment Platform", link: "/articles/how-tamaya-uses-linux" },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: "github", link: "https://github.com/bhbs/tamaya" },
    ],
  },
});
