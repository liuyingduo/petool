/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // WeChat-like color palette
        primary: {
          50: '#e7f3ff',
          100: '#c0ddff',
          200: '#95c5ff',
          300: '#66adff',
          400: '#3e95ff',
          500: '#07c160', // WeChat green
          600: '#06a652',
          700: '#058a44',
          800: '#046e36',
          900: '#035228',
        },
        dark: {
          bg: '#1e1e1e',
          surface: '#2d2d2d',
          border: '#3e3e3e',
          text: '#e0e0e0',
          textSecondary: '#a0a0a0',
        }
      }
    },
  },
  plugins: [],
}
