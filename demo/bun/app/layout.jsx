import "./globals.css";

export const metadata = {
  title: "demo-bun",
  description: "Next.js demo compiled with Bun",
};

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
