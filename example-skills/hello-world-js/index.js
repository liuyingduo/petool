#!/usr/bin/env node

/**
 * PETool Skill - Hello World (JavaScript)
 *
 * Skills receive parameters as JSON string as first argument
 * and should output result as JSON to stdout
 */

// Parse input parameters
const paramsJson = process.argv[2] || '{}';
const params = JSON.parse(paramsJson);

// Get the name parameter, default to "World"
const name = params.name || 'World';

// Execute skill logic
const result = {
  success: true,
  message: `Hello, ${name}!`,
  timestamp: new Date().toISOString(),
  skill: {
    id: 'hello-world-js',
    name: 'Hello World (JavaScript)',
    version: '1.0.0'
  }
};

// Output result as JSON
console.log(JSON.stringify(result, null, 2));
