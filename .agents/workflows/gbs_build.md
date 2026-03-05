---
description: Tizen gbs build workflow
---

# TizenClaw GBS Build Workflow

프로젝트 빌드 관련 변경 사항(코드 수정, CMakeLists.txt 수정, 패키징 스펙 변경)이 발생하면 아래 순서대로 자동 빌드를 수행하고 검증하세요.

1. **빌드를 실행합니다**: Tizen `gbs build`는 git repository의 commit된 소스를 기준으로 tarball을 만들지만, `--include-all` 옵션을 주면 커밋하지 않은 사항들도 포함하여 빌드합니다.
   명령어: `gbs build -A x86_64 --include-all`

2. **빌드 완료 확인**: 빌드가 정상적으로 완료되면 마지막에 `info: Done` 메시지가 출력됩니다. 이 메시지가 나타나면 빌드 성공입니다.

3. **빌드 로그 확인**:
   - 성공 시: `~/GBS-ROOT/local/repos/tizen/x86_64/logs/success/`
   - 실패 시: `~/GBS-ROOT/local/repos/tizen/x86_64/logs/fail/`

   위 디렉토리 아래에 생성되는 로그 파일을 통해 빌드 결과를 검증할 수 있습니다.

4. **주의: 파이프(`|`) 사용 금지**
   `gbs build` 명령어의 출력을 `| tail`, `| grep` 등 파이프로 필터링하지 마세요. 파이프가 출력을 버퍼링하여 빌드가 멈춘 것처럼 보이는 현상이 발생합니다.
   - ❌ `gbs build -A x86_64 --include-all 2>&1 | tail -50`
   - ✅ `gbs build -A x86_64 --include-all 2>&1`

   빌드 결과 확인이 필요한 경우, 빌드 완료 후 로그 파일을 직접 확인하세요.

// turbo-all
