export function greeting(name: string): string {
  return `Hello, ${name}!`;
}

export function initial(name: string): string {
  return name.slice(0, 1);
}

export function normalize(text: string): string {
  return text.trim();
}

export function shout(text: string): string {
  return text.toUpperCase();
}

export function banner(title: string): string {
  return `== ${title} ==`;
}

export function fullName(first: string, last: string): string {
  return first + " " + last;
}
