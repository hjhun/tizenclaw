Name:       tizenclaw
Summary:    TizenClaw Agent System Service App (Python Port)
Version:    1.0.0
Release:    1
Group:      System/Service
License:    Apache-2.0
Source0:    %{name}-%{version}.tar.gz
Source1001: %{name}.manifest

BuildRequires:  python3
BuildRequires:  python3-devel
BuildRequires:  python3-numpy
BuildRequires:  cmake

Requires:   python3
Requires:   python3-numpy

%description
TizenClaw Native Agent ported from C++ to Python to evaluate memory, speed, and storage footprints on Tizen.

%prep
%setup -q -n %{name}-%{version}
cp %{SOURCE1001} .

%build
%cmake .
%make_build

%install
rm -rf %{buildroot}
%make_install

# Auto-enable service on boot
mkdir -p %{buildroot}%{_unitdir}/multi-user.target.wants
mkdir -p %{buildroot}%{_unitdir}/sockets.target.wants
ln -sf ../tizenclaw.service %{buildroot}%{_unitdir}/multi-user.target.wants/tizenclaw.service
ln -sf ../tizenclaw-tool-executor.socket %{buildroot}%{_unitdir}/sockets.target.wants/
ln -sf ../tizenclaw-code-sandbox.socket %{buildroot}%{_unitdir}/sockets.target.wants/

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%manifest %{name}.manifest
%{_bindir}/tizenclaw-daemon
%{_bindir}/tizenclaw
%{_bindir}/tizenclaw-cli
%{_bindir}/tizenclaw-tool-executor
%{_bindir}/tizenclaw-code-sandbox
%{_unitdir}/tizenclaw.service
%{_unitdir}/tizenclaw-tool-executor.service
%{_unitdir}/tizenclaw-tool-executor.socket
%{_unitdir}/tizenclaw-code-sandbox.service
%{_unitdir}/tizenclaw-code-sandbox.socket
%{_unitdir}/multi-user.target.wants/tizenclaw.service
%{_unitdir}/sockets.target.wants/tizenclaw-tool-executor.socket
%{_unitdir}/sockets.target.wants/tizenclaw-code-sandbox.socket
/opt/usr/share/tizenclaw-python/*
/opt/usr/share/tizenclaw/scripts/run_standard_container.sh
