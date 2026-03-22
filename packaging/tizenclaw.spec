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
# Base dependencies from repo_config.ini

# Placeholder for additional independent RPMs to be built later
# BuildRequires: python3-grpcio
# BuildRequires: python3-protobuf
# BuildRequires: python3-onnxruntime

Requires:   python3
Requires:   python3-numpy

%description
TizenClaw Native Agent ported from C++ to Python to evaluate memory, speed, and storage footprints on Tizen.

%prep
%setup -q -n %{name}-%{version}
cp %{SOURCE1001} .

%build
# Pure python application doesn't require complex C/C++ compilation.
# Placeholder for future setup.py execution or script preparations.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}/opt/usr/share/tizenclaw-python
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/scripts
mkdir -p %{buildroot}%{_unitdir}/multi-user.target.wants

# Copy Python port source to the share directory
cp scripts/run_standard_container.sh %{buildroot}/opt/usr/share/tizenclaw/scripts/
chmod +x %{buildroot}/opt/usr/share/tizenclaw/scripts/run_standard_container.sh
cp -r src_py/* %{buildroot}/opt/usr/share/tizenclaw-python/
chmod +x %{buildroot}/opt/usr/share/tizenclaw-python/tizenclaw_daemon.py
chmod +x %{buildroot}/opt/usr/share/tizenclaw-python/tizenclaw_cli.py

# Link to bin directory
ln -sf /opt/usr/share/tizenclaw-python/tizenclaw_daemon.py %{buildroot}%{_bindir}/tizenclaw-daemon
ln -sf /opt/usr/share/tizenclaw-python/tizenclaw_daemon.py %{buildroot}%{_bindir}/tizenclaw
ln -sf /opt/usr/share/tizenclaw-python/tizenclaw_cli.py %{buildroot}%{_bindir}/tizenclaw-cli

# Systemd deployment
cp packaging/tizenclaw.service %{buildroot}%{_unitdir}/
ln -sf ../tizenclaw.service %{buildroot}%{_unitdir}/multi-user.target.wants/tizenclaw.service

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%manifest %{name}.manifest
%{_bindir}/tizenclaw-daemon
%{_bindir}/tizenclaw
%{_bindir}/tizenclaw-cli
%{_unitdir}/tizenclaw.service
%{_unitdir}/multi-user.target.wants/tizenclaw.service
/opt/usr/share/tizenclaw-python/*
/opt/usr/share/tizenclaw/scripts/run_standard_container.sh
