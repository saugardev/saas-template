import { RootProvider } from 'fumadocs-ui/provider/next';
import './global.css';
import { Inter } from 'next/font/google';
import type { Metadata } from 'next';

const inter = Inter({
  subsets: ['latin'],
});

export const metadata: Metadata = {
  metadataBase: new URL(process.env.DOCS_URL ?? 'http://localhost:3003'),
  title: {
    default: 'Agent SaaS Starter Docs',
    template: '%s · Agent SaaS Starter',
  },
  description: 'Set up and extend the SaaS, OAuth, OIDC, and MCP starter.',
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={inter.className} suppressHydrationWarning>
      <body className="flex flex-col min-h-screen">
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
