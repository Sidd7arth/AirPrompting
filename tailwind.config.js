/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./overlay.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Every colour resolves through a CSS variable so light/dark themes are
        // defined once in src/styles/globals.css rather than per-component.
        bg: "hsl(var(--bg) / <alpha-value>)",
        surface: "hsl(var(--surface) / <alpha-value>)",
        elevated: "hsl(var(--elevated) / <alpha-value>)",
        border: "hsl(var(--border) / <alpha-value>)",
        fg: "hsl(var(--fg) / <alpha-value>)",
        muted: "hsl(var(--muted) / <alpha-value>)",
        faint: "hsl(var(--faint) / <alpha-value>)",
        accent: "hsl(var(--accent) / <alpha-value>)",
        "accent-fg": "hsl(var(--accent-fg) / <alpha-value>)",
        danger: "hsl(var(--danger) / <alpha-value>)",
        success: "hsl(var(--success) / <alpha-value>)",
        warning: "hsl(var(--warning) / <alpha-value>)",
        recording: "hsl(var(--recording) / <alpha-value>)"
      },
      borderRadius: {
        xl: "0.75rem",
        "2xl": "1rem"
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "Segoe UI Variable Text",
          "Segoe UI",
          "Inter",
          "system-ui",
          "sans-serif"
        ],
        mono: [
          "ui-monospace",
          "SF Mono",
          "Cascadia Mono",
          "Menlo",
          "Consolas",
          "monospace"
        ]
      },
      fontSize: {
        "2xs": ["0.6875rem", { lineHeight: "1rem" }]
      },
      keyframes: {
        "fade-in": {
          from: { opacity: "0" },
          to: { opacity: "1" }
        },
        "overlay-in": {
          from: { opacity: "0", transform: "scale(0.98)" },
          to: { opacity: "1", transform: "scale(1)" }
        },
        shake: {
          "0%, 100%": { transform: "translateX(0)" },
          "20%, 60%": { transform: "translateX(-4px)" },
          "40%, 80%": { transform: "translateX(4px)" }
        },
        "pulse-soft": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.45" }
        }
      },
      animation: {
        "fade-in": "fade-in 150ms ease-out",
        "overlay-in": "overlay-in 180ms cubic-bezier(0.16, 1, 0.3, 1)",
        shake: "shake 260ms ease-in-out",
        "pulse-soft": "pulse-soft 1.4s ease-in-out infinite"
      }
    }
  },
  plugins: []
};
