import fs from 'fs';
import path from 'path';
import codegen from '@cosmwasm/ts-codegen';

// Function to execute the cargo command in a given directory
function executeCargoCommand(dir: string): string | undefined {
    const fileNamePattern = /Exported the full API as (.*)$/;
    try {
        // cd into the directory and execute the cargo command
        const { stdout } = Bun.spawnSync(['cargo', 'run', '--quiet', '--bin', 'schema'], {
            cwd: dir,
            stdout: 'pipe',
        })
        // Check stdout for which APIs were generated
        const stdoutLines = stdout.toString().split('\n');
        const fileName = stdoutLines
            .filter(line => fileNamePattern.test(line))
            .map(line => {
                const match = line.match(fileNamePattern);
                return match ? path.basename(match[1]).replace(/\.[^/.]+$/, "") : undefined;
            });

        return fileName[0];
    } catch (e) {
        return undefined;
    }
}

// Get the absolute path of the contracts directory
const contractsDir = path.resolve(import.meta.dir, '..', 'contracts');

// Read the directories in the contracts directory
const dirs = fs.readdirSync(contractsDir).map(dir => path.join(contractsDir, dir));

// Iterate over each directory and execute the cargo command
let contracts: { name: string, dir: string }[] = [];
dirs.forEach(dir => {
    const stat = fs.statSync(dir);
    if (stat.isDirectory()) {
        const fileName = executeCargoCommand(dir);
        if (fileName === undefined) {
            console.error(`Failed to generate schema for ${dir}`);
            return;
        }
        contracts.push({ name: fileName, dir: dir });
    }
});

console.log("Generated schemas: ");
contracts.forEach(contract => {
    console.log(`- ${contract.name}`);
});

const outPath = path.resolve(import.meta.dir, '..', '..', 'unstake.js', 'src', 'types');
fs.rmdirSync(outPath, { recursive: true });
fs.mkdirSync(outPath);
codegen({
    contracts: contracts,
    outPath: outPath,
    options: {
        useShorthandCtor: true,
        bundle: {
            bundleFile: 'index.ts',
            scope: 'contracts'
        },
        types: {
            enabled: true,
        },
        client: {
            enabled: false,
        },
        reactQuery: {
            enabled: false,
        },
        recoil: {
            enabled: false,
        },
        messageComposer: {
            enabled: false,
        },
        msgBuilder: {
            enabled: true,
        },
        useContractsHooks: {
            enabled: false,
        },
    }
}).then(() => {
    console.log('✨ all done!');
});
