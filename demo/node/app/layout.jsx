import "./globals.css";

export const metadata = {
  title: "demo-node",
  description: "Next.js demo compiled with Node SEA",
};

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
