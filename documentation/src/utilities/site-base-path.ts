const baseUrl = import.meta.env.BASE_URL;

export const siteBasePath = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
