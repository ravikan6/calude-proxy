// Simple dashboard app for Claude Code Proxy
class App {
    constructor() {
        this.apiBase = '/api';
        this.loadingElement = document.getElementById('loading');
        this.contentElement = document.getElementById('content');
        this.errorElement = document.getElementById('error');
        this.errorMessageElement = document.getElementById('error-message');
        this.currentView = 'dashboard';
        this.config = null;
        this.clients = [];
        this.providers = [];
        this.routes = [];
        this.versions = [];
    }

    async init() {
        try {
            await this.loadConfig();
            await this.loadClients();
            await this.loadProviders();
            await this.loadRoutes();
            await this.loadVersions();
            this.render();
        } catch (error) {
            this.showError('Failed to initialize dashboard: ' + error.message);
        }
    }

    async loadConfig() {
        try {
            const response = await fetch(`${this.apiBase}/config`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            this.config = await response.json();
        } catch (error) {
            throw error;
        }
    }

    async loadClients() {
        try {
            const response = await fetch(`${this.apiBase}/clients`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const data = await response.json();
            this.clients = data.data || [];
        } catch (error) {
            throw error;
        }
    }

    async loadProviders() {
        try {
            const response = await fetch(`${this.apiBase}/providers`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const data = await response.json();
            this.providers = data.data || [];
        } catch (error) {
            throw error;
        }
    }

    async loadRoutes() {
        try {
            const response = await fetch(`${this.apiBase}/routes`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const data = await response.json();
            this.routes = data.data || [];
        } catch (error) {
            throw error;
        }
    }

    async loadVersions() {
        try {
            const response = await fetch(`${this.apiBase}/versions`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const data = await response.json();
            this.versions = data.data || [];
        } catch (error) {
            throw error;
        }
    }

    async fetchApi(endpoint, options = {}) {
        try {
            const response = await fetch(`${this.apiBase}${endpoint}`, {
                headers: {
                    'Content-Type': 'application/json',
                    ...(options.headers || {})
                },
                ...options
            });

            if (!response.ok) {
                const errorData = await response.json();
                throw new Error(errorData.error || `HTTP error! status: ${response.status}`);
            }

            return await response.json();
        } catch (error) {
            throw error;
        }
    }

    showError(message) {
        this.errorMessageElement.textContent = message;
        this.errorElement.classList.remove('hidden');
        this.loadingElement.classList.add('hidden');
    }

    hideError() {
        this.errorElement.classList.add('hidden');
    }

    showLoading() {
        this.loadingElement.classList.remove('hidden');
        this.contentElement.classList.add('hidden');
    }

    hideLoading() {
        this.loadingElement.classList.add('hidden');
        this.contentElement.classList.remove('hidden');
    }

    async render() {
        try {
            this.hideError();
            this.showLoading();

            // Render the navigation
            this.contentElement.innerHTML = this.renderNavigation();

            // Render the current view
            switch (this.currentView) {
                case 'dashboard':
                    this.contentElement.innerHTML += this.renderDashboard();
                    break;
                case 'clients':
                    this.contentElement.innerHTML += this.renderClients();
                    break;
                case 'providers':
                    this.contentElement.innerHTML += this.renderProviders();
                    break;
                case 'routes':
                    this.contentElement.innerHTML += this.renderRoutes();
                    break;
                case 'versions':
                    this.contentElement.innerHTML += this.renderVersions();
                    break;
                default:
                    this.contentElement.innerHTML += this.renderDashboard();
            }

            // Add event listeners for navigation
            this.addNavigationListeners();

            this.hideLoading();
        } catch (error) {
            this.showError('Failed to render dashboard: ' + error.message);
        }
    }

    renderNavigation() {
        return `
            <nav class="bg-white shadow rounded-lg mb-8">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div class="flex justify-between h-16">
                        <div class="flex space-x-8">
                            <button data-view="dashboard" class="inline-flex items-center px-1 pt-1 border-b-2 ${this.currentView === 'dashboard' ? 'border-primary text-primary' : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'} text-sm font-medium">
                                Dashboard
                            </button>
                            <button data-view="clients" class="inline-flex items-center px-1 pt-1 border-b-2 ${this.currentView === 'clients' ? 'border-primary text-primary' : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'} text-sm font-medium">
                                Clients
                            </button>
                            <button data-view="providers" class="inline-flex items-center px-1 pt-1 border-b-2 ${this.currentView === 'providers' ? 'border-primary text-primary' : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'} text-sm font-medium">
                                Providers
                            </button>
                            <button data-view="routes" class="inline-flex items-center px-1 pt-1 border-b-2 ${this.currentView === 'routes' ? 'border-primary text-primary' : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'} text-sm font-medium">
                                Routes
                            </button>
                            <button data-view="versions" class="inline-flex items-center px-1 pt-1 border-b-2 ${this.currentView === 'versions' ? 'border-primary text-primary' : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'} text-sm font-medium">
                                Versions
                            </button>
                        </div>
                    </div>
                </div>
            </nav>
        `;
    }

    addNavigationListeners() {
        document.querySelectorAll('[data-view]').forEach(button => {
            button.addEventListener('click', async (e) => {
                this.currentView = e.target.getAttribute('data-view');
                await this.render();
            });
        });
    }

    renderDashboard() {
        return `
            <div class="space-y-6">
                <div class="bg-white rounded-lg shadow-md p-6">
                    <h2 class="text-xl font-bold mb-4">Welcome to Claude Code Proxy Dashboard</h2>
                    <p class="text-gray-600">
                        This dashboard allows you to manage your proxy configuration through a web interface.
                    </p>
                </div>

                <div class="bg-white rounded-lg shadow-md p-6">
                    <h2 class="text-xl font-bold mb-4">Current Status</h2>
                    <div class="space-y-4">
                        <div class="flex justify-between">
                            <span class="text-gray-600">Configuration Loaded:</span>
                            <span class="font-medium">${this.config.success ? 'Yes' : 'No'}</span>
                        </div>
                        ${this.config.success ? `
                            <div class="flex justify-between">
                                <span class="text-gray-600">Data Available:</span>
                                <span class="font-medium">Yes</span>
                            </div>
                        ` : ''}
                    </div>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                    <div class="bg-white rounded-lg shadow-md p-6">
                        <h3 class="text-lg font-bold mb-2">Clients</h3>
                        <p class="text-3xl font-bold text-primary">${this.clients.length}</p>
                    </div>
                    <div class="bg-white rounded-lg shadow-md p-6">
                        <h3 class="text-lg font-bold mb-2">Providers</h3>
                        <p class="text-3xl font-bold text-primary">${this.providers.length}</p>
                    </div>
                    <div class="bg-white rounded-lg shadow-md p-6">
                        <h3 class="text-lg font-bold mb-2">Routes</h3>
                        <p class="text-3xl font-bold text-primary">${this.routes.length}</p>
                    </div>
                </div>
            </div>
        `;
    }

    renderClients() {
        return `
            <div class="bg-white rounded-lg shadow-md p-6">
                <div class="flex justify-between items-center mb-6">
                    <h2 class="text-xl font-bold">Clients</h2>
                    <button id="add-client-btn" class="bg-primary hover:bg-primary-dark text-white font-medium py-2 px-4 rounded-md">
                        Add Client
                    </button>
                </div>

                <div class="overflow-x-auto">
                    <table class="min-w-full divide-y divide-gray-200">
                        <thead class="bg-gray-50">
                            <tr>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">ID</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Allowed Routes</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Requests/Minute</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Concurrent Requests</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Actions</th>
                            </tr>
                        </thead>
                        <tbody class="bg-white divide-y divide-gray-200">
                            ${this.clients.map(client => `
                                <tr>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900">${client.client_id}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${client.allowed_routes.join(', ')}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${client.requests_per_minute}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${client.concurrent_requests}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium">
                                        <button data-action="edit" data-id="${client.client_id}" class="text-indigo-600 hover:text-indigo-900 mr-2">Edit</button>
                                        <button data-action="delete" data-id="${client.client_id}" class="text-red-600 hover:text-red-900">Delete</button>
                                    </td>
                                </tr>
                            `).join('')}
                        </tbody>
                    </table>
                </div>
            </div>
        `;
    }

    renderProviders() {
        return `
            <div class="bg-white rounded-lg shadow-md p-6">
                <div class="flex justify-between items-center mb-6">
                    <h2 class="text-xl font-bold">Providers</h2>
                    <button id="add-provider-btn" class="bg-primary hover:bg-primary-dark text-white font-medium py-2 px-4 rounded-md">
                        Add Provider
                    </button>
                </div>

                <div class="overflow-x-auto">
                    <table class="min-w-full divide-y divide-gray-200">
                        <thead class="bg-gray-50">
                            <tr>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">ID</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Kind</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Endpoint</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Actions</th>
                            </tr>
                        </thead>
                        <tbody class="bg-white divide-y divide-gray-200">
                            ${this.providers.map(provider => `
                                <tr>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900">${provider.provider_id}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${provider.kind}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${provider.endpoint}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium">
                                        <button data-action="edit" data-id="${provider.provider_id}" class="text-indigo-600 hover:text-indigo-900 mr-2">Edit</button>
                                        <button data-action="delete" data-id="${provider.provider_id}" class="text-red-600 hover:text-red-900">Delete</button>
                                    </td>
                                </tr>
                            `).join('')}
                        </tbody>
                    </table>
                </div>
            </div>
        `;
    }

    renderRoutes() {
        return `
            <div class="bg-white rounded-lg shadow-md p-6">
                <div class="flex justify-between items-center mb-6">
                    <h2 class="text-xl font-bold">Routes</h2>
                    <button id="add-route-btn" class="bg-primary hover:bg-primary-dark text-white font-medium py-2 px-4 rounded-md">
                        Add Route
                    </button>
                </div>

                <div class="overflow-x-auto">
                    <table class="min-w-full divide-y divide-gray-200">
                        <thead class="bg-gray-50">
                            <tr>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">ID</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Models</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Actions</th>
                            </tr>
                        </thead>
                        <tbody class="bg-white divide-y divide-gray-200">
                            ${this.routes.map(route => `
                                <tr>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900">${route.route_id}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${route.models.join(', ')}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium">
                                        <button data-action="edit" data-id="${route.route_id}" class="text-indigo-600 hover:text-indigo-900 mr-2">Edit</button>
                                        <button data-action="delete" data-id="${route.route_id}" class="text-red-600 hover:text-red-900">Delete</button>
                                    </td>
                                </tr>
                            `).join('')}
                        </tbody>
                    </table>
                </div>
            </div>
        `;
    }

    renderVersions() {
        return `
            <div class="bg-white rounded-lg shadow-md p-6">
                <h2 class="text-xl font-bold mb-6">Configuration Versions</h2>

                <div class="overflow-x-auto">
                    <table class="min-w-full divide-y divide-gray-200">
                        <thead class="bg-gray-50">
                            <tr>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Version</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Created At</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Created By</th>
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Actions</th>
                            </tr>
                        </thead>
                        <tbody class="bg-white divide-y divide-gray-200">
                            ${this.versions.map(version => `
                                <tr>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900">${version.version}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${version.created_at}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">${version.created_by}</td>
                                    <td class="px-6 py-4 whitespace-nowrap text-sm font-medium">
                                        <button data-action="revert" data-version="${version.version}" class="text-indigo-600 hover:text-indigo-900">Revert</button>
                                    </td>
                                </tr>
                            `).join('')}
                        </tbody>
                    </table>
                </div>
            </div>
        `;
    }
}

// Export for use in HTML
window.App = App;