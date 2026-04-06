// TizenClaw Dashboard — Vanilla JS SPA
(function () {
    'use strict';

    const API = '';  // Same origin

    // --- Auth State ---
    let authToken =
        localStorage.getItem('admin_token') ||
        sessionStorage.getItem('admin_token');

    function persistAdminToken(token) {
        authToken = token;
        localStorage.setItem('admin_token', token);
        sessionStorage.setItem('admin_token', token);
    }

    function clearAdminToken() {
        authToken = null;
        localStorage.removeItem('admin_token');
        sessionStorage.removeItem('admin_token');
    }

    function getAuthHeaders() {
        return authToken
            ? { 'Authorization': 'Bearer ' + authToken }
            : {};
    }

    // --- Navigation ---
    const navItems =
        document.querySelectorAll('.nav-item');
    const pages =
        document.querySelectorAll('.page');
    let metricsInterval = null;

    function navigateTo(page) {
        navItems.forEach(n =>
            n.classList.remove('active'));
        pages.forEach(p =>
            p.classList.remove('active'));

        // Stop dashboard auto-refresh when leaving
        if (page !== 'dashboard' && metricsInterval) {
            clearInterval(metricsInterval);
            metricsInterval = null;
        }

        const navEl =
            document.getElementById('nav-' + page);
        const pageEl =
            document.getElementById('page-' + page);
        if (navEl) navEl.classList.add('active');
        if (pageEl) pageEl.classList.add('active');

        if (page === 'dashboard') loadDashboard();
        else if (page === 'sessions') loadSessions();
        else if (page === 'tasks') loadTasks();
        else if (page === 'logs') loadLogs();
        else if (page === 'chat') loadChatSessions();
        else if (page === 'ota') loadOta();
        else if (page === 'admin') loadAdmin();
    }

    navItems.forEach(item => {
        item.addEventListener('click', () => {
            navigateTo(item.dataset.page);
        });
    });

    // --- API Helpers ---
    async function apiFetch(endpoint, opts) {
        try {
            const headers = Object.assign(
                {}, getAuthHeaders(),
                (opts && opts.headers) || {});
            const res = await fetch(
                API + '/api/' + endpoint,
                Object.assign({}, opts, { headers }));
            const data = await res.json();
            data.__http_status = res.status;
            if (res.status === 401) {
                handleAdminUnauthorized(
                    data.error || 'Session expired');
            }
            return data;
        } catch (e) {
            console.error('API error:', e);
            return null;
        }
    }

    async function apiPost(endpoint, body) {
        return apiFetch(endpoint, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify(body)
        });
    }

    async function apiDelete(endpoint, body) {
        const opts = { method: 'DELETE' };
        if (body !== undefined) {
            opts.headers = {
                'Content-Type': 'application/json'
            };
            opts.body = JSON.stringify(body);
        }
        return apiFetch(endpoint, opts);
    }

    // --- Date Breadcrumb Navigator ---
    const MONTHS = [
        'Jan', 'Feb', 'Mar', 'Apr',
        'May', 'Jun', 'Jul', 'Aug',
        'Sep', 'Oct', 'Nov', 'Dec'
    ];

    class DateNav {
        constructor(elementId, onSelect) {
            this.el = document.getElementById(
                elementId);
            this.onSelect = onSelect;
            this.dates = [];
            this.level = 'year';
            this.selYear = null;
            this.selMonth = null;
            this.selDay = null;
        }

        setDates(dateStrings) {
            this.dates = (dateStrings || [])
                .slice().sort().reverse();
            this.level = 'year';
            this.selYear = null;
            this.selMonth = null;
            this.selDay = null;
            this.render();
        }

        getYears() {
            const s = new Set();
            this.dates.forEach(d =>
                s.add(d.substring(0, 4)));
            return [...s].sort().reverse();
        }

        getMonths(year) {
            const s = new Set();
            this.dates.forEach(d => {
                if (d.substring(0, 4) === year)
                    s.add(d.substring(5, 7));
            });
            return [...s].sort().reverse();
        }

        getDays(year, month) {
            return this.dates
                .filter(d =>
                    d.substring(0, 4) === year &&
                    d.substring(5, 7) === month)
                .map(d => d.substring(8, 10))
                .sort().reverse();
        }

        render() {
            if (!this.el) return;
            let html =
                '<span class="date-nav-label">' +
                '📅 Browse</span>';

            if (this.level === 'year') {
                const years = this.getYears();
                if (years.length === 0) {
                    html += '<span class=' +
                        '"date-nav-chip" ' +
                        'style="cursor:default;' +
                        'opacity:0.5">' +
                        'No dates</span>';
                } else {
                    years.forEach(y => {
                        html +=
                            '<span class=' +
                            '"date-nav-chip" ' +
                            'data-year="' +
                            y + '">' +
                            y + '</span>';
                    });
                }
            } else if (this.level === 'month') {
                html +=
                    '<span class="date-nav-chip' +
                    ' breadcrumb" data-reset=' +
                    '"year">' +
                    'All Years</span>' +
                    '<span class="date-nav-sep"' +
                    '>›</span>' +
                    '<span class="date-nav-chip' +
                    ' active">' +
                    this.selYear + '</span>' +
                    '<span class="date-nav-sep"' +
                    '>›</span>';
                const months =
                    this.getMonths(this.selYear);
                months.forEach(m => {
                    const mi = parseInt(m, 10) - 1;
                    html +=
                        '<span class=' +
                        '"date-nav-chip" ' +
                        'data-month="' + m +
                        '">' +
                        MONTHS[mi] +
                        '</span>';
                });
            } else if (this.level === 'day') {
                html +=
                    '<span class="date-nav-chip' +
                    ' breadcrumb" data-reset=' +
                    '"year">' +
                    'All Years</span>' +
                    '<span class="date-nav-sep"' +
                    '>›</span>' +
                    '<span class="date-nav-chip' +
                    ' breadcrumb" data-reset=' +
                    '"month">' +
                    this.selYear + '</span>' +
                    '<span class="date-nav-sep"' +
                    '>›</span>' +
                    '<span class="date-nav-chip' +
                    ' active">' +
                    MONTHS[
                        parseInt(this.selMonth,
                            10) - 1] +
                    '</span>' +
                    '<span class="date-nav-sep"' +
                    '>›</span>';
                const days = this.getDays(
                    this.selYear, this.selMonth);
                days.forEach(d => {
                    const full =
                        this.selYear + '-' +
                        this.selMonth + '-' + d;
                    const cls =
                        (this.selDay === d)
                            ? 'date-nav-chip active'
                            : 'date-nav-chip';
                    html +=
                        '<span class="' + cls +
                        '" data-day="' + d +
                        '" data-full="' +
                        full + '">' +
                        parseInt(d, 10) +
                        '</span>';
                });
            }

            // Show All button when filtered
            if (this.selYear || this.selDay) {
                html += '<span class=' +
                    '"date-nav-all" ' +
                    'data-reset="all">' +
                    '✕ Show All</span>';
            }

            this.el.innerHTML = html;
            this.bind();
        }

        bind() {
            if (!this.el) return;
            const self = this;
            this.el.querySelectorAll(
                '[data-year]').forEach(el => {
                    el.addEventListener(
                        'click', () => {
                            self.selYear =
                                el.dataset.year;
                            self.level = 'month';
                            self.render();
                        });
                });
            this.el.querySelectorAll(
                '[data-month]').forEach(el => {
                    el.addEventListener(
                        'click', () => {
                            self.selMonth =
                                el.dataset.month;
                            self.level = 'day';
                            self.selDay = null;
                            self.render();
                            self.onSelect(null);
                        });
                });
            this.el.querySelectorAll(
                '[data-day]').forEach(el => {
                    el.addEventListener(
                        'click', () => {
                            self.selDay =
                                el.dataset.day;
                            self.render();
                            self.onSelect(
                                el.dataset.full);
                        });
                });
            this.el.querySelectorAll(
                '[data-reset]').forEach(el => {
                    el.addEventListener(
                        'click', () => {
                            const r =
                                el.dataset.reset;
                            if (r === 'year' ||
                                r === 'all') {
                                self.selYear = null;
                                self.selMonth = null;
                                self.selDay = null;
                                self.level = 'year';
                            } else if (
                                r === 'month') {
                                self.selMonth = null;
                                self.selDay = null;
                                self.level =
                                    'month';
                            }
                            self.render();
                            if (r === 'all')
                                self.onSelect(null);
                        });
                });
        }

        getFilter() {
            if (!this.selYear) return null;
            if (!this.selMonth)
                return this.selYear;
            if (!this.selDay)
                return this.selYear + '-' +
                    this.selMonth;
            return this.selYear + '-' +
                this.selMonth + '-' +
                this.selDay;
        }
    }

    // --- Dashboard ---
    async function loadDashboard() {
        if (metricsInterval)
            clearInterval(metricsInterval);
        await refreshMetrics();
        metricsInterval = setInterval(refreshMetrics, 5000);
    }

    function fmtTokens(n) {
        if (n == null) return '—';
        if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
        if (n >= 1000)    return (n / 1000).toFixed(1) + 'K';
        return String(n);
    }

    async function refreshMetrics() {
        const [m, sessions, tasks] = await Promise.all([
            apiFetch('metrics'),
            apiFetch('sessions'),
            apiFetch('tasks'),
        ]);

        const s = id => document.getElementById(id);

        if (m) {
            const connected = m.agent_connected === true;

            // Sidebar agent status indicator
            const dot = s('status-dot');
            const label = s('status-label');
            if (dot)   dot.className = 'status-dot ' + (connected ? 'running' : 'disconnected');
            if (label) label.textContent = connected ? 'Agent running' : 'Agent offline';

            // LLM usage section badge
            const badge = s('usage-source-badge');
            if (badge) {
                badge.textContent = connected ? 'live' : 'offline';
                badge.style.background = connected
                    ? 'rgba(5,150,105,0.1)' : 'rgba(220,38,38,0.08)';
                badge.style.color = connected ? 'var(--success)' : 'var(--danger)';
                badge.style.borderColor = connected
                    ? 'rgba(5,150,105,0.15)' : 'rgba(220,38,38,0.15)';
            }

            // Primary stats
            if (s('stat-status')) {
                const txt = m.status || '—';
                s('stat-status').textContent = txt;
                const icon = s('stat-status').closest('.stat-card')
                    && s('stat-status').closest('.stat-card').querySelector('.stat-icon');
                if (icon) {
                    icon.className = 'stat-icon ' +
                        (txt === 'running' ? 'stat-icon--green' :
                         txt === 'disconnected' ? 'stat-icon--red' : 'stat-icon--amber');
                }
            }
            s('stat-uptime') &&
                (s('stat-uptime').textContent =
                    (m.uptime && m.uptime.formatted) || '—');

            // System health
            s('stat-memory') &&
                (s('stat-memory').textContent =
                    (m.memory && m.memory.vm_rss_kb)
                        ? (m.memory.vm_rss_kb / 1024).toFixed(1) + ' MB' : '—');
            s('stat-cpu') &&
                (s('stat-cpu').textContent =
                    (m.cpu && m.cpu.load_1m != null)
                        ? m.cpu.load_1m.toFixed(2) : '—');
            s('stat-threads') && (s('stat-threads').textContent = m.threads || '—');
            s('stat-pid')     && (s('stat-pid').textContent = m.pid || '—');

            // LLM counters
            s('stat-errors') &&
                (s('stat-errors').textContent =
                    (m.counters && m.counters.errors != null)
                        ? m.counters.errors : '—');
            s('stat-llm') &&
                (s('stat-llm').textContent =
                    (m.counters && m.counters.llm_calls != null)
                        ? m.counters.llm_calls : (connected ? '0' : '—'));
            s('stat-tools') &&
                (s('stat-tools').textContent =
                    (m.counters && m.counters.tool_calls != null)
                        ? m.counters.tool_calls : '—');

            // Token usage (new)
            if (m.tokens) {
                s('stat-prompt-tokens')     && (s('stat-prompt-tokens').textContent     = fmtTokens(m.tokens.prompt));
                s('stat-completion-tokens') && (s('stat-completion-tokens').textContent = fmtTokens(m.tokens.completion));
                s('stat-cache-read')        && (s('stat-cache-read').textContent        = fmtTokens(m.tokens.cache_read));
                s('stat-cache-write')       && (s('stat-cache-write').textContent       = fmtTokens(m.tokens.cache_write));
            } else {
                ['stat-prompt-tokens','stat-completion-tokens',
                 'stat-cache-read','stat-cache-write'].forEach(id => {
                    s(id) && (s(id).textContent = connected ? '0' : '—');
                });
            }
        }

        if (sessions && s('stat-sessions'))
            s('stat-sessions').textContent = sessions.length;
        if (tasks && s('stat-tasks'))
            s('stat-tasks').textContent = tasks.length;
    }

    // --- Sessions ---
    let sessionDateNav = null;
    let allSessions = null;

    async function loadSessions(filterDate) {
        const data = await apiFetch('sessions');
        const list =
            document.getElementById('session-list');
        const viewer =
            document.getElementById('session-viewer');
        viewer.style.display = 'none';
        list.style.display = '';

        allSessions = data || [];

        // Init date nav once
        if (!sessionDateNav) {
            sessionDateNav = new DateNav(
                'session-date-nav',
                function (date) {
                    renderSessions(date);
                });
        }
        // Collect unique dates
        const dates = allSessions.map(
            s => s.date).filter(Boolean);
        sessionDateNav.setDates(
            [...new Set(dates)]);

        renderSessions(filterDate || null);
    }

    function renderSessions(filterDate) {
        const list =
            document.getElementById('session-list');
        let items = allSessions || [];

        if (filterDate) {
            items = items.filter(
                s => s.date === filterDate);
        } else {
            // If nav has partial filter
            // (year or year-month)
            const f = sessionDateNav
                ? sessionDateNav.getFilter()
                : null;
            if (f) {
                items = items.filter(
                    s => s.date &&
                        s.date.startsWith(f));
            }
        }

        if (items.length === 0) {
            list.innerHTML =
                '<p class="empty-state">' +
                'No sessions found</p>';
            return;
        }

        list.innerHTML = items.map(s => {
            const sizeKB =
                (s.size_bytes / 1024).toFixed(1);
            const modified = s.modified ?
                new Date(s.modified * 1000)
                    .toLocaleString() : '—';
            return '<div class="card-item ' +
                'clickable" data-session-id="' +
                escHtml(s.id) + '">' +
                '<div class="card-item-title">' +
                escHtml(s.title || s.id) + '</div>' +
                '<div class="card-item-meta">' +
                escHtml(s.id) + ' · ' +
                sizeKB + ' KB · ' +
                modified + '</div></div>';
        }).join('');

        list.querySelectorAll('.card-item')
            .forEach(card => {
                card.addEventListener(
                    'click', () => {
                        showSessionDetail(
                            card.dataset
                                .sessionId);
                    });
            });
    }

    async function showSessionDetail(id) {
        const list =
            document.getElementById('session-list');
        const viewer =
            document.getElementById('session-viewer');
        const title = document.getElementById(
            'session-viewer-title');
        const content = document.getElementById(
            'session-viewer-content');

        list.style.display = 'none';
        viewer.style.display = '';
        title.textContent = id;
        content.textContent = 'Loading...';

        const resp =
            await apiFetch('sessions/' + id);
        if (resp && resp.content) {
            content.textContent = resp.content;
        } else {
            content.textContent =
                'Failed to load session.';
        }
    }

    document.getElementById('session-back')
        .addEventListener('click', () => {
            document.getElementById('session-viewer')
                .style.display = 'none';
            document.getElementById('session-list')
                .style.display = '';
        });

    // --- Tasks ---
    let taskDateNav = null;
    let allTasks = null;
    let currentTaskFile = null;
    let selectedTaskIds = new Set();

    const taskSelectAllBtn =
        document.getElementById('task-select-all');
    const taskDeleteSelectedBtn =
        document.getElementById('task-delete-selected');
    const taskSelectionMeta =
        document.getElementById('task-selection-meta');
    const taskDeleteCurrentBtn =
        document.getElementById('task-delete-current');

    async function loadTasks(filterDate) {
        const data = await apiFetch('tasks');
        const list =
            document.getElementById('task-list');
        const viewer =
            document.getElementById('task-viewer');
        viewer.style.display = 'none';
        list.style.display = '';
        currentTaskFile = null;

        allTasks = data || [];
        const availableTaskIds = new Set(
            allTasks.map(t => t.id).filter(Boolean));
        selectedTaskIds.forEach(id => {
            if (!availableTaskIds.has(id)) {
                selectedTaskIds.delete(id);
            }
        });

        // Init date nav once
        if (!taskDateNav) {
            taskDateNav = new DateNav(
                'task-date-nav',
                function (date) {
                    renderTasks(date);
                });
        }
        const dates = allTasks.map(
            t => t.date).filter(Boolean);
        taskDateNav.setDates(
            [...new Set(dates)]);

        renderTasks(filterDate || null);
        formatTaskSelectionMeta();
    }

    function renderTasks(filterDate) {
        const list =
            document.getElementById('task-list');
        let items = allTasks || [];

        if (filterDate) {
            items = items.filter(
                t => t.date === filterDate);
        } else {
            const f = taskDateNav
                ? taskDateNav.getFilter()
                : null;
            if (f) {
                items = items.filter(
                    t => t.date &&
                        t.date.startsWith(f));
            }
        }

        if (items.length === 0) {
            list.innerHTML =
                '<p class="empty-state">' +
                'No tasks found</p>';
            return;
        }

        list.innerHTML = items.map(t => {
            const modified = t.modified ?
                new Date(t.modified * 1000)
                    .toLocaleString() : '';
            const checked = selectedTaskIds
                .has(t.id) ? ' checked' : '';
            return '<div class="card-item task-card-item ' +
                'clickable" data-task-file="' +
                escHtml(t.file) + '" data-task-id="' +
                escHtml(t.id) + '">' +
                '<label class="chat-session-check">' +
                '<input type="checkbox" data-task-select="' +
                escHtml(t.id) + '"' + checked + ' />' +
                '<span></span></label>' +
                '<div class="card-item-title">' +
                escHtml(t.title || t.file) + '</div>' +
                '<div class="card-item-meta">' +
                (modified ? modified + ' · '
                    : '') +
                escHtml(t.file) + ' · ' +
                escHtml(
                    t.content_preview || '') +
                '</div></div>';
        }).join('');

        list.querySelectorAll('.card-item')
            .forEach(card => {
                card.addEventListener(
                    'click', (event) => {
                        if (event.target.closest(
                            '[data-task-select]')) {
                            return;
                        }
                        showTaskDetail(
                            card.dataset
                                .taskFile);
                    });
            });
        list.querySelectorAll('[data-task-select]')
            .forEach(input => {
                input.addEventListener('change', () => {
                    const id = input.dataset.taskSelect;
                    if (!id) return;
                    if (input.checked) {
                        selectedTaskIds.add(id);
                    } else {
                        selectedTaskIds.delete(id);
                    }
                    formatTaskSelectionMeta();
                });
            });
    }

    async function showTaskDetail(file) {
        const list =
            document.getElementById('task-list');
        const viewer =
            document.getElementById('task-viewer');
        const title = document.getElementById(
            'task-viewer-title');
        const content = document.getElementById(
            'task-viewer-content');

        list.style.display = 'none';
        viewer.style.display = '';
        title.textContent = file;
        content.textContent = 'Loading...';
        currentTaskFile = file;

        const resp =
            await apiFetch('tasks/' + file);
        if (resp && resp.content) {
            content.textContent = resp.content;
        } else {
            content.textContent =
                'Failed to load task.';
        }
    }

    function formatTaskSelectionMeta() {
        if (!taskSelectionMeta) return;
        const count = selectedTaskIds.size;
        taskSelectionMeta.textContent = count
            ? ('선택된 작업 ' + count + '개')
            : '선택된 작업 없음';
    }

    async function deleteTasks(ids) {
        const filteredIds = (ids || [])
            .filter(Boolean);
        if (!filteredIds.length) return;
        if (!window.confirm(
            '선택한 작업을 삭제할까요?')) {
            return;
        }

        let resp = null;
        if (filteredIds.length === 1) {
            resp = await apiDelete(
                'tasks/' +
                encodeURIComponent(filteredIds[0]));
        } else {
            resp = await apiDelete(
                'tasks', {
                    ids: filteredIds
                });
        }

        if (!resp || !resp.deleted_ids) {
            window.alert('작업 삭제에 실패했습니다.');
            return;
        }

        resp.deleted_ids.forEach(id => {
            selectedTaskIds.delete(id);
            if (currentTaskFile === id ||
                currentTaskFile === id + '.md') {
                currentTaskFile = null;
                document.getElementById('task-viewer')
                    .style.display = 'none';
                document.getElementById('task-list')
                    .style.display = '';
            }
        });
        await loadTasks(taskDateNav
            ? taskDateNav.getFilter()
            : null);
    }

    document.getElementById('task-back')
        .addEventListener('click', () => {
            document.getElementById('task-viewer')
                .style.display = 'none';
            document.getElementById('task-list')
                .style.display = '';
        });
    if (taskSelectAllBtn) {
        taskSelectAllBtn.addEventListener(
            'click', () => {
                const taskIds = (allTasks || [])
                    .map(task => task.id)
                    .filter(Boolean);
                if (selectedTaskIds.size ===
                    taskIds.length) {
                    selectedTaskIds.clear();
                } else {
                    selectedTaskIds =
                        new Set(taskIds);
                }
                renderTasks(taskDateNav
                    ? taskDateNav.getFilter()
                    : null);
                formatTaskSelectionMeta();
            });
    }
    if (taskDeleteSelectedBtn) {
        taskDeleteSelectedBtn.addEventListener(
            'click', async () => {
                await deleteTasks(Array.from(
                    selectedTaskIds));
            });
    }
    if (taskDeleteCurrentBtn) {
        taskDeleteCurrentBtn.addEventListener(
            'click', async () => {
                if (!currentTaskFile) return;
                const taskId = currentTaskFile
                    .endsWith('.md')
                    ? currentTaskFile.slice(0, -3)
                    : currentTaskFile;
                await deleteTasks([taskId]);
            });
    }

    // --- Logs ---
    let logDateNav = null;

    async function loadLogs(dateStr) {
        // Init date nav once
        if (!logDateNav) {
            logDateNav = new DateNav(
                'log-date-nav',
                function (date) {
                    loadLogContent(date);
                });
            // Load available dates
            const datesResp =
                await apiFetch('logs/dates');
            if (datesResp && datesResp.dates) {
                logDateNav.setDates(
                    datesResp.dates);
            }
        }

        // Load today by default
        loadLogContent(dateStr || null);
    }

    async function loadLogContent(dateStr) {
        const logEl =
            document.getElementById('log-content');
        logEl.textContent = 'Loading...';

        const endpoint = dateStr
            ? 'logs?date=' +
            encodeURIComponent(dateStr)
            : 'logs';
        const data = await apiFetch(endpoint);

        if (!data || data.length === 0) {
            logEl.textContent = dateStr
                ? 'No logs for ' + dateStr
                : 'No logs available.';
            return;
        }

        logEl.textContent =
            data.map(l =>
                '### ' + (l.label || l.file || 'Log') +
                '\n' + l.content)
                .join('\n\n');
    }

    // --- Chat ---
    const chatInput =
        document.getElementById('chat-input');
    const chatSend =
        document.getElementById('chat-send');
    const chatMessages =
        document.getElementById('chat-messages');
    const chatSessionList =
        document.getElementById('chat-session-list');
    const chatSessionMeta =
        document.getElementById('chat-session-meta');
    const chatNewSessionBtn =
        document.getElementById('chat-new-session');
    const chatSelectAllBtn =
        document.getElementById('chat-select-all');
    const chatDeleteSelectedBtn =
        document.getElementById(
            'chat-delete-selected');
    const chatSelectionMeta =
        document.getElementById(
            'chat-selection-meta');
    let currentChatSessionId = null;
    let chatSessionsCache = [];
    let selectedChatSessionIds = new Set();

    function formatChatSessionMeta() {
        if (!chatSessionMeta) return;
        if (!currentChatSessionId) {
            chatSessionMeta.textContent =
                '새 대화 중입니다. 첫 메시지를 보내면 세션이 생성됩니다.';
            return;
        }
        chatSessionMeta.textContent =
            '세션 ' + currentChatSessionId +
            ' 대화를 이어가는 중입니다.';
    }

    function resetChatMessages() {
        if (!chatMessages) return;
        chatMessages.innerHTML =
            '<div class="chat-welcome">' +
            'Type a message to start chatting ' +
            'with TizenClaw.</div>';
    }

    function updateChatSelectionMeta() {
        if (!chatSelectionMeta) return;
        const count = selectedChatSessionIds.size;
        if (count === 0) {
            chatSelectionMeta.textContent =
                '선택된 세션 없음';
            return;
        }
        chatSelectionMeta.textContent =
            count + '개 세션 선택됨';
    }

    function selectChatSession(sessionId) {
        currentChatSessionId = sessionId || null;
        formatChatSessionMeta();
        if (!chatSessionList) return;
        chatSessionList.querySelectorAll(
            '.chat-session-item').forEach(item => {
                item.classList.toggle('active',
                    item.dataset.sessionId ===
                    currentChatSessionId);
            });
    }

    function renderChatSessionList() {
        if (!chatSessionList) return;
        if (!chatSessionsCache.length) {
            chatSessionList.innerHTML =
                '<p class="empty-state">' +
                'No previous chats yet.</p>';
            selectedChatSessionIds.clear();
            updateChatSelectionMeta();
            return;
        }

        chatSessionList.innerHTML = chatSessionsCache
            .map(session => {
                const isActive =
                    session.id === currentChatSessionId
                        ? ' active' : '';
                const isChecked =
                    selectedChatSessionIds.has(
                        session.id)
                        ? ' checked' : '';
                const preview = escHtml(
                    session.content_preview ||
                    'No preview available.');
                const modified = session.modified
                    ? new Date(session.modified * 1000)
                        .toLocaleString()
                    : '—';
                return '<div class="chat-session-item' +
                    isActive + '" data-session-id="' +
                    escHtml(session.id) + '">' +
                    '<label class="chat-session-check">' +
                    '<input type="checkbox" ' +
                    'data-chat-select="' +
                    escHtml(session.id) + '"' +
                    isChecked + '>' +
                    '</label>' +
                    '<div class="chat-session-body">' +
                    '<div class="chat-session-title">' +
                    escHtml(session.title ||
                        session.id) + '</div>' +
                    '<div class="chat-session-preview">' +
                    preview + '</div>' +
                    '<div class="chat-session-meta">' +
                    escHtml(session.id) + ' · ' +
                    modified + ' · ' +
                    (session.message_count || 0) +
                    ' msgs</div></div>' +
                    '<button class="chat-session-delete" ' +
                    'data-chat-delete="' +
                    escHtml(session.id) + '">' +
                    '삭제</button></div>';
            }).join('');

        chatSessionList.querySelectorAll(
            '.chat-session-item').forEach(item => {
                item.addEventListener('click',
                    async (event) => {
                        if (event.target.closest(
                            '[data-chat-delete]') ||
                            event.target.closest(
                                '[data-chat-select]')) {
                            return;
                        }
                        await loadChatSessionDetail(
                            item.dataset.sessionId);
                    });
            });
        chatSessionList.querySelectorAll(
            '[data-chat-select]').forEach(box => {
                box.addEventListener('change',
                    (event) => {
                        const id =
                            event.target.dataset
                                .chatSelect;
                        if (event.target.checked) {
                            selectedChatSessionIds
                                .add(id);
                        } else {
                            selectedChatSessionIds
                                .delete(id);
                        }
                        updateChatSelectionMeta();
                    });
            });
        chatSessionList.querySelectorAll(
            '[data-chat-delete]').forEach(btn => {
                btn.addEventListener('click',
                    async (event) => {
                        event.stopPropagation();
                        await deleteChatSessions([
                            btn.dataset
                                .chatDelete
                        ]);
                    });
            });
        updateChatSelectionMeta();
    }

    async function loadChatSessions() {
        if (!chatSessionList) return;
        chatSessionList.innerHTML =
            '<p class="empty-state">Loading...</p>';
        const sessions = await apiFetch('sessions');
        if (!Array.isArray(sessions)) {
            chatSessionsCache = [];
            chatSessionList.innerHTML =
                '<p class="empty-state">' +
                'Failed to load previous chats.</p>';
            formatChatSessionMeta();
            return;
        }
        chatSessionsCache = sessions;
        selectedChatSessionIds.forEach(id => {
            if (!chatSessionsCache.some(
                session => session.id === id)) {
                selectedChatSessionIds
                    .delete(id);
            }
        });
        renderChatSessionList();
        formatChatSessionMeta();
    }

    async function deleteChatSessions(ids) {
        const filteredIds = (ids || [])
            .filter(Boolean);
        if (!filteredIds.length) return;
        if (!window.confirm(
            '선택한 세션 기록을 삭제할까요?')) {
            return;
        }

        let resp = null;
        if (filteredIds.length === 1) {
            resp = await apiDelete(
                'sessions/' +
                encodeURIComponent(filteredIds[0]));
        } else {
            resp = await apiDelete(
                'sessions', {
                    ids: filteredIds
                });
        }

        if (!resp || !resp.deleted_ids) {
            window.alert('세션 삭제에 실패했습니다.');
            return;
        }

        resp.deleted_ids.forEach(id => {
            selectedChatSessionIds.delete(id);
            if (currentChatSessionId === id) {
                currentChatSessionId = null;
                resetChatMessages();
            }
        });
        await loadChatSessions();
        selectChatSession(currentChatSessionId);
    }

    async function loadChatSessionDetail(sessionId) {
        if (!chatMessages) return;
        const resp = await apiFetch('sessions/' +
            encodeURIComponent(sessionId));
        if (!resp || !Array.isArray(resp.messages)) {
            addChatMsg('assistant',
                'Failed to load session history.');
            return;
        }

        chatMessages.innerHTML = '';
        resp.messages.forEach(message => {
            addChatMsg(message.role, message.text);
        });
        selectChatSession(sessionId);
    }

    function addChatMsg(role, text) {
        if (!chatMessages) return;
        const welcome =
            chatMessages.querySelector('.chat-welcome');
        if (welcome) welcome.remove();

        const el = document.createElement('div');
        el.className = 'chat-msg ' + role;
        el.textContent = text;
        chatMessages.appendChild(el);
        chatMessages.scrollTop =
            chatMessages.scrollHeight;
    }

    async function sendChat() {
        if (!chatInput || !chatMessages) return;
        const prompt = chatInput.value.trim();
        if (!prompt) return;
        const sessionId = currentChatSessionId;

        addChatMsg('user', prompt);
        chatInput.value = '';
        
        // Removed chatSend.disabled = true; to allow concurrent/overlapping requests

        // Show thinking indicator specific to this request
        const thinkingId = 'think-' + Date.now() + Math.random().toString(36).substr(2, 5);
        const thinking = document.createElement('div');
        thinking.className = 'chat-thinking';
        thinking.id = thinkingId;
        thinking.innerHTML =
            '<span class="chat-thinking-dot"></span>' +
            '<span class="chat-thinking-dot"></span>' +
            '<span class="chat-thinking-dot"></span>';
        chatMessages.appendChild(thinking);
        chatMessages.scrollTop =
            chatMessages.scrollHeight;

        try {
            const resp = await apiPost('chat', {
                prompt: prompt,
                session_id: sessionId
            });

            // Remove this specific thinking indicator
            const indicator = document.getElementById(thinkingId);
            if (indicator) indicator.remove();

            if (resp && resp.session_id) {
                selectChatSession(resp.session_id);
            }

            if (resp && resp.response) {
                addChatMsg('assistant', resp.response);
                await loadChatSessions();
            } else {
                addChatMsg('assistant',
                    (resp && resp.error) ||
                    'Error: no response from agent.');
            }
        } catch (err) {
            const indicator = document.getElementById(thinkingId);
            if (indicator) indicator.remove();
            addChatMsg('assistant', 'Error: connection failed.');
        }
    }

    if (chatSend) {
        chatSend.addEventListener('click', sendChat);
    }
    if (chatNewSessionBtn) {
        chatNewSessionBtn.addEventListener('click', () => {
            currentChatSessionId = null;
            resetChatMessages();
            selectChatSession(null);
        });
    }
    if (chatSelectAllBtn) {
        chatSelectAllBtn.addEventListener(
            'click', () => {
                if (selectedChatSessionIds.size ===
                    chatSessionsCache.length) {
                    selectedChatSessionIds.clear();
                } else {
                    selectedChatSessionIds =
                        new Set(chatSessionsCache
                            .map(session =>
                                session.id));
                }
                renderChatSessionList();
            });
    }
    if (chatDeleteSelectedBtn) {
        chatDeleteSelectedBtn.addEventListener(
            'click', async () => {
                await deleteChatSessions(
                    Array.from(
                        selectedChatSessionIds));
            });
    }
    if (chatInput) {
        chatInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                sendChat();
            }
        });
    }

    // ==========================
    // Admin Page
    // ==========================

    const CONFIG_LABELS = {
        'llm_config.json': 'LLM Configuration',
        'telegram_config.json': 'Telegram Bot',
        'slack_config.json': 'Slack Integration',
        'discord_config.json': 'Discord Bot',
        'webhook_config.json': 'Webhook Routes',
        'tool_policy.json': 'Tool Policy',
        'agent_roles.json': 'Agent Roles',
        'tunnel_config.json': 'Tunnel Configuration',
        'web_search_config.json': 'Web Search'
    };
    const CONFIG_DESCRIPTIONS = {
        'llm_config.json': 'Manage model backends, token limits, and sampling options.',
        'telegram_config.json': 'Configure the Telegram bot token and channel bindings.',
        'slack_config.json': 'Configure Slack app tokens, bot tokens, and channel bindings.',
        'discord_config.json': 'Adjust Discord bot credentials and connection settings.',
        'webhook_config.json': 'Control webhook endpoints and routing policies.',
        'tool_policy.json': 'Manage allowed tools and execution policies.',
        'agent_roles.json': 'Define agent roles and prompt routing behavior.',
        'tunnel_config.json': 'Manage tunnel endpoints and authentication tokens.',
        'web_search_config.json': 'Configure search providers and search options.'
    };
    let adminConfigsCache = [];
    let activeConfigName = null;
    let activeConfigParsed = null;

    async function loadAdmin() {
        if (!authToken) {
            showLoginForm();
            return;
        }
        if (await ensureAdminSession()) {
            showAdminPanel();
        }
    }

    function showLoginForm(message) {
        document.getElementById('admin-login')
            .style.display = '';
        document.getElementById('admin-panel')
            .style.display = 'none';
        closeConfigModal();
        document.getElementById(
            'login-error').textContent =
            message || '';
    }

    function showAdminPanel() {
        document.getElementById('admin-login')
            .style.display = 'none';
        document.getElementById('admin-panel')
            .style.display = '';
        loadConfigs();
    }

    function handleAdminUnauthorized(message) {
        clearAdminToken();
        showLoginForm(message || 'Session expired');
    }

    async function ensureAdminSession() {
        if (!authToken) return false;
        const resp = await apiFetch('auth/session');
        return !!(resp && resp.status === 'ok');
    }

    // --- Login ---
    document.getElementById('admin-login-btn')
        .addEventListener('click', doLogin);
    document.getElementById('admin-password')
        .addEventListener('keydown', (e) => {
            if (e.key === 'Enter') doLogin();
        });

    async function doLogin() {
        const pw = document.getElementById(
            'admin-password').value;
        const errEl = document.getElementById(
            'login-error');
        errEl.textContent = '';

        if (!pw) {
            errEl.textContent = 'Password required';
            return;
        }

        const resp = await apiPost(
            'auth/login', { password: pw });

        if (resp && resp.status === 'ok') {
            persistAdminToken(resp.token);
            document.getElementById(
                'admin-password').value = '';
            showAdminPanel();
            showToast('Admin session ready',
                'success');
        } else {
            errEl.textContent =
                (resp && resp.error) ||
                'Login failed';
        }
    }

    // --- Logout ---
    document.getElementById('admin-logout-btn')
        .addEventListener('click', async () => {
            await apiPost('auth/logout', {});
            clearAdminToken();
            showLoginForm();
        });

    // --- Password Change ---
    document.getElementById('admin-change-pw-btn')
        .addEventListener('click', () => {
            const f = document.getElementById(
                'pw-change-form');
            f.style.display =
                f.style.display === 'none' ? '' : 'none';
        });

    document.getElementById('pw-cancel-btn')
        .addEventListener('click', () => {
            document.getElementById('pw-change-form')
                .style.display = 'none';
        });

    document.getElementById('pw-save-btn')
        .addEventListener('click', async () => {
            const cur = document.getElementById(
                'pw-current').value;
            const nw = document.getElementById(
                'pw-new').value;
            const msg = document.getElementById(
                'pw-change-msg');

            if (!cur || !nw) {
                msg.textContent = 'Fill in both fields';
                msg.style.color = 'var(--danger)';
                return;
            }

            const resp = await apiPost(
                'auth/change_password', {
                current_password: cur,
                new_password: nw
            });

            if (resp && resp.status === 'ok') {
                msg.textContent = 'Password changed!';
                msg.style.color = 'var(--success)';
                document.getElementById(
                    'pw-current').value = '';
                document.getElementById(
                    'pw-new').value = '';
                setTimeout(() => {
                    document.getElementById(
                        'pw-change-form').style.display =
                        'none';
                    msg.textContent = '';
                }, 2000);
            } else {
                msg.textContent =
                    (resp && resp.error) || 'Failed';
                msg.style.color = 'var(--danger)';
            }
        });

    // --- Config Management ---
    async function loadConfigs() {
        const list = document.getElementById(
            'config-list');
        const data = await apiFetch('config/list');

        if (!data || !data.configs) {
            list.innerHTML =
                '<p class="empty-state">' +
                'Failed to load configs</p>';
            return;
        }

        adminConfigsCache = data.configs.slice();
        list.innerHTML = data.configs.map(c => {
            const label =
                CONFIG_LABELS[c.name] || c.name;
            const statusClass =
                c.exists ? 'exists' : 'missing';
            const statusText =
                c.exists ? '● Active' : '○ Sample';

            return '<button type="button" class="config-card"' +
                ' data-config="' + escHtml(c.name) + '">' +
                '<div class="config-card-header">' +
                '<div class="config-card-copy">' +
                '<span class="config-card-title">' +
                escHtml(label) + '</span>' +
                '<p class="config-card-desc">' +
                escHtml(CONFIG_DESCRIPTIONS[c.name] ||
                    'Configuration editor') + '</p>' +
                '</div>' +
                '<div class="config-card-side">' +
                '<span class="config-card-status ' +
                statusClass + '">' +
                statusText + '</span>' +
                '<span class="config-card-open">Open</span>' +
                '</div></div></button>';
        }).join('');

        list.querySelectorAll('.config-card')
            .forEach(card => {
                card.addEventListener('click', () => {
                    openConfigModal(
                        card.dataset.config);
                });
            });
    }

    async function fetchConfigContent(name) {
        const resp = await apiFetch(
            'config/' + name);

        if (resp && resp.status === 'ok') {
            return {
                ok: true,
                exists: true,
                content: resp.content
            };
        }
        if (resp && resp.sample) {
            return {
                ok: true,
                exists: false,
                content: resp.sample,
                message: 'No config found — sample loaded'
            };
        }
        return {
            ok: false,
            error: (resp && resp.error) ||
                'Load failed'
        };
    }

    function tryParseJson(content) {
        try {
            return JSON.parse(content);
        } catch (e) {
            return null;
        }
    }

    function renderConfigField(key, value) {
        const type = Array.isArray(value)
            ? 'array'
            : value === null
                ? 'null'
                : typeof value;

        if (type === 'boolean') {
            return '<label class="config-field">' +
                '<span class="config-field-label">' +
                escHtml(key) + '</span>' +
                '<select class="config-field-input"' +
                ' data-config-key="' + escHtml(key) + '"' +
                ' data-config-type="boolean">' +
                '<option value="true"' +
                (value ? ' selected' : '') +
                '>true</option>' +
                '<option value="false"' +
                (!value ? ' selected' : '') +
                '>false</option></select></label>';
        }

        if (type === 'number') {
            return '<label class="config-field">' +
                '<span class="config-field-label">' +
                escHtml(key) + '</span>' +
                '<input type="number" class="config-field-input"' +
                ' data-config-key="' + escHtml(key) + '"' +
                ' data-config-type="number" value="' +
                escHtml(String(value)) + '"></label>';
        }

        if (type === 'object' || type === 'array') {
            return '<label class="config-field">' +
                '<span class="config-field-label">' +
                escHtml(key) + '</span>' +
                '<textarea class="config-field-input config-field-code"' +
                ' data-config-key="' + escHtml(key) + '"' +
                ' data-config-type="json">' +
                escHtml(JSON.stringify(value, null, 2)) +
                '</textarea></label>';
        }

        return '<label class="config-field">' +
            '<span class="config-field-label">' +
            escHtml(key) + '</span>' +
            '<textarea class="config-field-input"' +
            ' data-config-key="' + escHtml(key) + '"' +
            ' data-config-type="string">' +
            escHtml(value === null ? '' : String(value)) +
            '</textarea></label>';
    }

    function renderConfigStructuredEditor() {
        const fields = document.getElementById(
            'config-modal-fields');
        const helper = document.getElementById(
            'config-modal-helper');

        if (!activeConfigParsed ||
            typeof activeConfigParsed !== 'object' ||
            Array.isArray(activeConfigParsed)) {
            fields.innerHTML =
                '<p class="empty-state">Structured editing is available only for JSON objects.</p>';
            helper.textContent =
                'Use the raw editor to update the full document.';
            return;
        }

        const entries =
            Object.entries(activeConfigParsed);
        helper.textContent =
            'Update top-level fields here, then save the configuration.';
        fields.innerHTML = entries.length
            ? entries.map(([key, value]) =>
                renderConfigField(key, value)).join('')
            : '<p class="empty-state">No editable fields were found.</p>';
    }

    function setConfigModalMode(mode) {
        const structured = document.getElementById(
            'config-modal-structured');
        const raw = document.getElementById(
            'config-modal-raw-wrap');
        const structuredTab = document.getElementById(
            'config-tab-structured');
        const rawTab = document.getElementById(
            'config-tab-raw');
        const canUseStructured = !!(
            activeConfigParsed &&
            typeof activeConfigParsed === 'object' &&
            !Array.isArray(activeConfigParsed));

        if (mode === 'structured' &&
            !canUseStructured) {
            mode = 'raw';
        }

        if (mode === 'raw' &&
            structuredTab.classList.contains(
                'active')) {
            try {
                document.getElementById(
                    'config-modal-raw').value =
                    collectStructuredConfig();
            } catch (e) {
                document.getElementById(
                    'config-modal-msg').textContent =
                    'Unable to switch to raw view: ' +
                    e.message;
                document.getElementById(
                    'config-modal-msg').className =
                    'config-modal-msg error';
                return;
            }
        }

        if (mode === 'structured' &&
            rawTab.classList.contains('active')) {
            const parsed = tryParseJson(
                document.getElementById(
                    'config-modal-raw').value);
            if (!parsed ||
                typeof parsed !== 'object' ||
                Array.isArray(parsed)) {
                document.getElementById(
                    'config-modal-msg').textContent =
                    'Structured view requires a JSON object.';
                document.getElementById(
                    'config-modal-msg').className =
                    'config-modal-msg error';
                return;
            }
            activeConfigParsed = parsed;
            renderConfigStructuredEditor();
        }

        structured.style.display =
            mode === 'structured' ? '' : 'none';
        raw.style.display =
            mode === 'raw' ? '' : 'none';
        structuredTab.classList.toggle(
            'active', mode === 'structured');
        rawTab.classList.toggle(
            'active', mode === 'raw');
        structuredTab.disabled =
            !canUseStructured;
    }

    async function openConfigModal(name) {
        const modal = document.getElementById(
            'config-modal');
        const msg = document.getElementById(
            'config-modal-msg');
        const title = document.getElementById(
            'config-modal-title');
        const file = document.getElementById(
            'config-modal-name');
        const status = document.getElementById(
            'config-modal-status');
        const format = document.getElementById(
            'config-modal-format');
        const raw = document.getElementById(
            'config-modal-raw');

        activeConfigName = name;
        title.textContent =
            CONFIG_LABELS[name] || name;
        file.textContent = name;
        msg.textContent = 'Loading...';
        msg.className = 'config-modal-msg';
        modal.classList.add('open');
        document.body.classList.add('modal-open');

        const loaded = await fetchConfigContent(name);
        if (!loaded.ok) {
            msg.textContent = loaded.error;
            msg.className =
                'config-modal-msg error';
            return;
        }

        raw.value = loaded.content || '';
        activeConfigParsed = tryParseJson(raw.value);
        status.textContent = loaded.exists
            ? 'Active'
            : 'Sample';
        status.className = 'config-chip ' +
            (loaded.exists ? 'success' :
                'warning');
        format.textContent = activeConfigParsed
            ? 'JSON'
            : 'TEXT';
        renderConfigStructuredEditor();
        setConfigModalMode('structured');

        if (loaded.message) {
            msg.textContent = loaded.message;
            msg.className =
                'config-modal-msg warning';
        } else {
            msg.textContent = '';
            msg.className =
                'config-modal-msg';
        }
    }

    function closeConfigModal() {
        const modal = document.getElementById(
            'config-modal');
        if (modal) {
            modal.classList.remove('open');
        }
        document.body.classList.remove('modal-open');
        activeConfigName = null;
        activeConfigParsed = null;
    }

    function collectStructuredConfig() {
        const next = {};
        const inputs = document.querySelectorAll(
            '#config-modal-fields [data-config-key]');

        for (const input of inputs) {
            const key = input.dataset.configKey;
            const type = input.dataset.configType;
            let value = input.value;

            if (type === 'boolean') {
                value = value === 'true';
            } else if (type === 'number') {
                if (value.trim() === '' ||
                    Number.isNaN(Number(value))) {
                    throw new Error(
                        key + ' must be numeric');
                }
                value = Number(value);
            } else if (type === 'json') {
                value = JSON.parse(value);
            }

            next[key] = value;
        }

        return JSON.stringify(next, null, 2);
    }

    async function saveConfig(name) {
        const msg = document.getElementById(
            'config-modal-msg');
        const structuredTab = document.getElementById(
            'config-tab-structured');
        const rawEditor = document.getElementById(
            'config-modal-raw');
        let content = rawEditor.value;

        try {
            if (structuredTab.classList.contains(
                'active')) {
                content = collectStructuredConfig();
            } else if (tryParseJson(content)) {
                content = JSON.stringify(
                    JSON.parse(content), null, 2);
            }
        } catch (e) {
            msg.textContent =
                'Invalid config: ' + e.message;
            msg.className =
                'config-modal-msg error';
            return;
        }

        msg.textContent = 'Saving...';
        msg.className = 'config-modal-msg';

        const resp = await apiPost(
            'config/' + name, { content: content });

        if (resp && resp.status === 'ok') {
            rawEditor.value = content;
            activeConfigParsed =
                tryParseJson(content);
            msg.textContent =
                'Saved successfully!';
            msg.className =
                'config-modal-msg success';
            await loadConfigs();
            showToast(
                (CONFIG_LABELS[name] || name) +
                ' saved',
                'success'
            );
        } else {
            msg.textContent =
                (resp && resp.error) || 'Save failed';
            msg.className =
                'config-modal-msg error';
        }
    }

    document.getElementById('config-modal-close')
        .addEventListener('click',
            closeConfigModal);
    document.getElementById(
        'config-modal-backdrop')
        .addEventListener('click',
            closeConfigModal);
    document.getElementById(
        'config-tab-structured')
        .addEventListener('click', () => {
            setConfigModalMode('structured');
        });
    document.getElementById('config-tab-raw')
        .addEventListener('click', () => {
            setConfigModalMode('raw');
        });
    document.getElementById('config-modal-reload')
        .addEventListener('click', () => {
            if (activeConfigName) {
                openConfigModal(activeConfigName);
            }
        });
    document.getElementById('config-modal-save')
        .addEventListener('click', () => {
            if (activeConfigName) {
                saveConfig(activeConfigName);
            }
        });
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            closeConfigModal();
        }
    });

    // ==========================
    // OTA Updates
    // ==========================

    function loadOta() {
        const list =
            document.getElementById('ota-list');
        const status =
            document.getElementById('ota-status');
        list.innerHTML =
            '<p class="empty-state">' +
            'Click "Check for Updates" to scan ' +
            'for available skill updates.</p>';
        status.textContent = '';
    }

    document.getElementById('ota-check-btn')
        .addEventListener('click', async () => {
            const list =
                document.getElementById('ota-list');
            const status =
                document.getElementById('ota-status');
            status.textContent = 'Checking...';
            status.className = 'ota-status';
            list.innerHTML =
                '<p class="empty-state">' +
                'Scanning...</p>';

            const data =
                await apiFetch('ota/check');

            if (!data || data.error) {
                status.textContent =
                    data ? data.error : 'Failed';
                status.className =
                    'ota-status error';
                list.innerHTML =
                    '<p class="empty-state">' +
                    escHtml(
                        data ? data.error
                            : 'Check failed'
                    ) + '</p>';
                return;
            }

            const count = data.available_count || 0;
            status.textContent = count > 0
                ? count + ' update(s) available'
                : 'All skills up to date';
            status.className = count > 0
                ? 'ota-status warning'
                : 'ota-status success';

            if (!data.updates ||
                data.updates.length === 0) {
                list.innerHTML =
                    '<p class="empty-state">' +
                    'No skills in manifest</p>';
                return;
            }

            list.innerHTML = data.updates.map(
                u => {
                    const badge = u.update_available
                        ? '<span class="ota-badge ' +
                          'update">Update</span>'
                        : '<span class="ota-badge ' +
                          'current">Current</span>';
                    const actions =
                        u.update_available
                            ? '<button class="' +
                              'btn-outline ota-update"' +
                              ' data-skill="' +
                              escHtml(u.name) +
                              '">Update</button>'
                            : '';
                    return '<div class="card-item ' +
                        'ota-card">' +
                        '<div class="' +
                        'card-item-title">' +
                        escHtml(u.name) +
                        ' ' + badge + '</div>' +
                        '<div class="' +
                        'card-item-meta">' +
                        'Local: ' +
                        escHtml(u.local_version) +
                        ' → Remote: ' +
                        escHtml(u.remote_version) +
                        '</div>' +
                        '<div class="ota-actions">' +
                        actions +
                        '<button class="' +
                        'btn-outline ota-rollback"' +
                        ' data-skill="' +
                        escHtml(u.name) +
                        '">Rollback</button>' +
                        '</div></div>';
                }
            ).join('');

            // Bind update buttons
            list.querySelectorAll('.ota-update')
                .forEach(btn => {
                    btn.addEventListener(
                        'click', () => {
                            otaUpdateSkill(
                                btn.dataset.skill);
                        });
                });

            // Bind rollback buttons
            list.querySelectorAll('.ota-rollback')
                .forEach(btn => {
                    btn.addEventListener(
                        'click', () => {
                            otaRollbackSkill(
                                btn.dataset.skill);
                        });
                });
        });

    async function otaUpdateSkill(name) {
        const status =
            document.getElementById('ota-status');
        status.textContent =
            'Updating ' + name + '...';
        status.className = 'ota-status';

        const resp = await apiPost(
            'ota/update', { skill: name });

        if (resp && resp.status === 'updated') {
            status.textContent = name +
                ' updated to v' +
                resp.new_version;
            status.className =
                'ota-status success';
        } else if (
            resp && resp.status === 'up_to_date') {
            status.textContent = name +
                ' is already up to date';
            status.className =
                'ota-status success';
        } else {
            status.textContent =
                'Update failed: ' +
                (resp ? resp.error : 'unknown');
            status.className = 'ota-status error';
        }
    }

    async function otaRollbackSkill(name) {
        if (!confirm(
            'Rollback ' + name +
            ' to previous version?')) return;

        const status =
            document.getElementById('ota-status');
        status.textContent =
            'Rolling back ' + name + '...';
        status.className = 'ota-status';

        const resp = await apiPost(
            'ota/rollback', { skill: name });

        if (resp &&
            resp.status === 'rolled_back') {
            status.textContent = name +
                ' rolled back to v' +
                resp.restored_version;
            status.className =
                'ota-status success';
        } else {
            status.textContent =
                'Rollback failed: ' +
                (resp ? resp.error : 'unknown');
            status.className = 'ota-status error';
        }
    }

    // --- Utility ---
    function escHtml(s) {
        const div = document.createElement('div');
        div.textContent = s;
        return div.innerHTML;
    }

    // --- Toast Notifications ---
    function showToast(msg, type, durationMs) {
        durationMs = durationMs || 3000;
        const container =
            document.getElementById('toast-container');
        if (!container) return;
        const el = document.createElement('div');
        el.className = 'toast' + (type ? ' ' + type : '');
        el.textContent = msg;
        container.appendChild(el);
        setTimeout(function () {
            el.style.animation =
                'toastOut 0.22s ease forwards';
            setTimeout(function () {
                el.remove();
            }, 230);
        }, durationMs);
    }
    window._showToast = showToast;

    // --- Initial Load ---
    formatChatSessionMeta();
    loadDashboard();
})();
