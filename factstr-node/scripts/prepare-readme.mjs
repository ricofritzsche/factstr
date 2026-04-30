import { copyFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const packageDirectory = dirname(scriptDirectory);
const sourcePath = join(packageDirectory, 'NPM.md');
const outputPath = join(packageDirectory, 'README.md');

if (!existsSync(sourcePath)) {
  console.error(`Missing npm package README source: ${sourcePath}`);
  process.exit(1);
}

copyFileSync(sourcePath, outputPath);
console.log(`Prepared package README from ${sourcePath}`);
