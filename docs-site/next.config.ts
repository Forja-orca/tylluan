import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  allowedDevOrigins: ["127.0.0.1", "localhost", "localhost:3010", "127.0.0.1:3010"],
};

export default nextConfig;
