import { fileURLToPath } from "node:url";

/** @type {import("next").NextConfig} */
const nextConfig = {
  adapterPath: fileURLToPath(import.meta.resolve("next-sea")),
};

export default nextConfig;
