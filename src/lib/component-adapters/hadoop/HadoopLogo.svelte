<script lang="ts">
  let { class: className = "" }: { class?: string } = $props();
</script>

<span class="logo {className}" aria-hidden="true"></span>

<style>
  .logo {
    display: block;
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    background: currentColor;
    -webkit-mask: url("/icons/apache-hadoop.svg") center / contain no-repeat;
    mask: url("/icons/apache-hadoop.svg") center / contain no-repeat;
  }
</style>
